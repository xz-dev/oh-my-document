//! Store: on-disk layout, single-writer lock, expected-version preconditions,
//! and atomic state publication (design E-2/E-3).
//!
//! Immutable records are written to their real per-ID paths
//! (`commits/<id>.toml`, `versions/<id>.toml`, `content/<sha256>`), each
//! fsync'd, then a new `state.toml` is written under a temp name, fsync'd,
//! atomically renamed over `state.toml`, and the directory fsync'd. The
//! rename is the only visible transition: a crash before it leaves the old
//! state intact, and leftover immutable files are diagnosed orphans — never
//! mistaken for dangling published history.
//!
//! Crash injection uses a production [`PublishProbe`] seam: the default is a
//! no-op; the test harness supplies a fault-injecting implementation. No
//! library code depends on the test module.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::records::commit::Commit;
use crate::records::time::OsRng;
use crate::records::version::SourceVersion;
use crate::relations::dirty::DirtyState;

/// Publication stages that a [`PublishProbe`] may observe or fail.
/// Order matches the real write sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    WriteRecords,
    SyncRecords,
    WriteTempState,
    SyncTempState,
    RenameState,
    SyncDir,
}

/// Probe called at each publication boundary. Production uses [`NoProbe`];
/// tests substitute a fault injector. Returning `false` aborts the publish.
pub trait PublishProbe {
    fn at(&mut self, _stage: Stage) -> bool {
        true
    }
}

/// Default probe: never fails.
pub struct NoProbe;
impl PublishProbe for NoProbe {}

/// Lock held on `write.lock` for the duration of a mutation.
/// Released by the OS on process exit — no stale-lockfile recovery.
pub struct WriteLock {
    _file: fslock::LockFile,
}

/// On-disk mutable index. Everything readers need is reachable from this;
/// nothing else on disk is authoritative "current" state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    /// Per-store publication marker (monotonic sequence, not a clock).
    pub publication: u64,
    /// Operation ID of the last published update (for lost-response checks).
    pub operation_id: Option<String>,
    /// node chain key → tip commit id (hex).
    #[serde(default)]
    pub tips: BTreeMap<String, String>,
    /// mount tree: parent node key → ordered child node keys.
    #[serde(default)]
    pub mounts: BTreeMap<String, Vec<String>>,
    /// source-version id → binding record id (immutable binding file).
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
    /// registration name → registration record id.
    #[serde(default)]
    pub registrations: BTreeMap<String, String>,
    /// All published record IDs not yet collected by gc. reset moves tips but
    /// never removes from this set — only explicit gc may shrink it.
    #[serde(default)]
    pub retained: Vec<String>,
    /// Per-node dirty/obligation state (node key → DirtyState). Persisted so
    /// `unclean` stacks survive process restarts and `verify` can read them.
    #[serde(default)]
    pub dirty: BTreeMap<String, DirtyState>,
    /// Per-node open ATOMIC-begin stack (node key → open BEGIN commit ids,
    /// outermost-first). `verify` fails while any is non-empty.
    #[serde(default)]
    pub open_blocks: BTreeMap<String, Vec<String>>,
    /// commit id → commit id it reset *from* (the previous tip moved off).
    /// Lets `list --dangling` distinguish still-reachable commits from ones
    /// reset withdrew (dangling) vs never-published orphans.
    #[serde(default)]
    pub reset_from: BTreeMap<String, String>,
    /// link_id → link instance (persistent identity). A link stays valid
    /// until its creation commit is withdrawn; identical endpoints/direction
    /// can coexist as distinct instances with distinct link_ids.
    #[serde(default)]
    pub links: BTreeMap<String, Link>,
    /// link_id → set of pending obligation change-keys awaiting adapt.
    /// Each change on a link is its own obligation; adapt selects which to
    /// clear. Never merged by endpoint.
    #[serde(default)]
    pub link_pending: BTreeMap<String, BTreeSet<String>>,
    /// node key → set of flat project-local tag names. Dir tags inherit to
    /// members (additive, deduped); same-named tags across projects never
    /// share identity — the name is scoped to this store.
    #[serde(default)]
    pub tags: BTreeMap<String, BTreeSet<String>>,
    /// Named tag-link rules: `spec->code` (one-way) or `spec<->code`
    /// (two-way) + severity (`warn`|`fail`, default fail). A rule is a named
    /// check item — never a new marker lifecycle nor a verify gate.
    #[serde(default)]
    pub tag_rules: BTreeMap<String, TagRule>,
    /// Independent 128-bit store identity — distinct from project_id and
    /// business commit ids. A copied dir must register a NEW store_id (and
    /// complete external-reference registration) before it may publish or gc;
    /// an unregistered copy reads history for diagnostics only.
    #[serde(default)]
    pub store_id: String,
    /// Whether this store copy completed external-reference registration.
    /// false = history/diagnostic reads only; business writes + gc refuse.
    #[serde(default)]
    pub activated: bool,
    /// peer store_id → registration (alias → locator), for cross-store links.
    #[serde(default)]
    pub peers: BTreeMap<String, PeerReg>,
    /// Inbound protection credentials a peer persisted against our targets
    /// before ITS record published: credential_id → InboundCred.
    #[serde(default)]
    pub inbound: BTreeMap<String, InboundCred>,
}

/// A registered peer store (alias → locator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReg {
    pub store_id: String,
    pub locator: String,
}

/// An inbound protection credential: peer persisted its identity + record id
/// + the exact protected target before publishing its own business record.
/// The credential alone never proves the link — the peer's live record does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundCred {
    /// Peer store_id that owns the protecting record.
    pub peer_store_id: String,
    /// The peer's pending business record id.
    pub record_id: String,
    /// Our exact protected target (commit/version id).
    pub target: String,
}

/// A declared tag-link rule checked by `check`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRule {
    /// e.g. "spec->code" or "spec<->code".
    pub rule: String,
    /// "warn" or "fail" (default fail).
    pub level: String,
    /// Explicitly skipped by the user? skip never confirms content.
    #[serde(default)]
    pub skip: bool,
}

/// A persisted link instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub link_id: String,
    pub source: String,
    pub target: String,
    /// The commit that created it (identity separate from the link itself).
    pub created_by: String,
}

/// What a caller observed before writing — checked under the lock.
#[derive(Debug, Clone, Default)]
pub struct Expected {
    /// Publication marker the caller last saw (must equal current).
    pub publication: Option<u64>,
    /// Specific tips that must still hold (node key → commit id hex).
    pub tips: BTreeMap<String, String>,
    /// Specific registrations that must still hold.
    pub registrations: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("lock conflict")]
    Lock,
    #[error("version conflict: {0}")]
    Conflict(String),
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("record invalid: {0}")]
    Record(String),
}

/// A metadata store rooted at a directory.
pub struct Store {
    root: PathBuf,
    lock: Option<WriteLock>,
    state: State,
}

impl Store {
    /// Open (or initialize) a store at `root` in the default layout.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        for d in ["commits", "versions", "content", "bindings", "registrations", "notes", "inbound", "pending"] {
            fs::create_dir_all(root.join(d))?;
        }
        let state_path = root.join("state.toml");
        // Ensure the published-record manifest exists (rebuildable: it
        // lists every commit id the state retains — reset never deletes it).
        let manifest_path = root.join("published");
        if !manifest_path.exists() {
            fs::write(&manifest_path, "")?;
        }
        let state = if state_path.exists() {
            let s = fs::read_to_string(&state_path)?;
            toml::from_str(&s).map_err(|e| StoreError::Record(e.to_string()))?
        } else {
            let st = State::default();
            fs::write(&state_path, toml::to_string(&st).map_err(|e| StoreError::Record(e.to_string()))?)?;
            st
        };
        // A fresh store (never had a store_id) gets one + is activated.
        let mut state = state;
        if state.store_id.is_empty() {
            state.store_id = crate::records::cross::new_store_id(&OsRng);
            state.activated = true;
            fs::write(&state_path, toml::to_string(&state).map_err(|e| StoreError::Record(e.to_string()))?)?;
        }
        Ok(Self { root: root.to_path_buf(), lock: None, state })
    }

    /// Acquire the single-writer lock. Fails fast on contention — no waiting,
    /// no stale-lockfile prying, no re-read-and-retry. Idempotent within one
    /// Store: a composite operation may call lock() at each step and the
    /// already-held lock is reused, not contended with itself.
    pub fn lock(&mut self) -> Result<(), StoreError> {
        if self.lock.is_some() {
            return Ok(()); // already held by this store
        }
        let path = self.root.join("write.lock");
        let mut lf = fslock::LockFile::open(&path)?;
        if !lf.try_lock()? {
            return Err(StoreError::Lock);
        }
        self.lock = Some(WriteLock { _file: lf });
        Ok(())
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Read a commit record by id (immutable `commits/<id>.toml`).
    pub fn read_commit(&self, id: &str) -> Result<crate::records::commit::Commit, StoreError> {
        let s = fs::read_to_string(self.root.join(format!("commits/{id}.toml")))?;
        toml::from_str(&s).map_err(|e| StoreError::Record(e.to_string()))
    }

    /// Read a version record by id (`versions/<id>.toml`).
    pub fn read_version(&self, id: &str) -> Result<crate::records::version::SourceVersion, StoreError> {
        let s = fs::read_to_string(self.root.join(format!("versions/{id}.toml")))?;
        toml::from_str(&s).map_err(|e| StoreError::Record(e.to_string()))
    }

    /// Read stored content bytes by sha256 (`content/<sha256>`).
    pub fn read_content(&self, sha256: &str) -> Result<Vec<u8>, StoreError> {
        Ok(fs::read(self.root.join(format!("content/{sha256}")))?)
    }

    /// The store root path (for resolving `commits/`, `versions/`, `content/`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Persist a metadata-only state update (binding/registration revisions
    /// that aren't business commits) via the same atomic state swap.
    pub fn set_state(&mut self, st: State) -> Result<(), StoreError> {
        let path = self.root.join("state.toml");
        let tmp = self.root.join("state.toml.tmp");
        let txt = toml::to_string(&st).map_err(|e| StoreError::Record(e.to_string()))?;
        fs::write(&tmp, txt)?;
        fs::rename(&tmp, &path)?;
        self.state = st;
        Ok(())
    }

    /// Verify caller-observed preconditions under the held lock.
    /// Any mismatch aborts the write — never silently uses the newer value.
    pub fn check_expected(&self, exp: &Expected) -> Result<(), StoreError> {
        if let Some(p) = exp.publication {
            if p != self.state.publication {
                return Err(StoreError::Conflict(format!(
                    "publication {p} != current {}",
                    self.state.publication
                )));
            }
        }
        for (node, tip) in &exp.tips {
            match self.state.tips.get(node) {
                Some(cur) if cur == tip => {}
                _ => return Err(StoreError::Conflict(format!("tip for {node} changed"))),
            }
        }
        for (name, reg) in &exp.registrations {
            match self.state.registrations.get(name) {
                Some(cur) if cur == reg => {}
                _ => return Err(StoreError::Conflict(format!("registration {name} changed"))),
            }
        }
        Ok(())
    }

    /// Write one immutable file and fsync it. `rel` is the path under the
    /// store root (e.g. `commits/<id>.toml`). Fails if the destination
    /// already exists — immutable records are never overwritten.
    fn write_immutable(&self, rel: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        if p.exists() {
            // content/<sha256> is content-addressed: identical bytes are the
            // same file. Re-observing equal content shares it, not an error.
            if rel.starts_with("content/") {
                return Ok(());
            }
            return Err(StoreError::Record(format!("immutable record exists: {rel}")));
        }
        let mut f = File::create(&p)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    }

    /// Publish a logical update: immutable per-ID records first, then the
    /// atomic state swap. The rename over `state.toml` is the single point
    /// where the world changes.
    pub fn publish(
        &mut self,
        probe: &mut dyn PublishProbe,
        commit: &Commit,
        commit_id: &str,
        version: Option<&SourceVersion>,
        content: Option<&[u8]>,
        new_state: State,
    ) -> Result<(), StoreError> {
        if !probe.at(Stage::WriteRecords) {
            return Err(StoreError::Io(io::Error::new(io::ErrorKind::Other, "probe abort: write records")));
        }
        let commit_bytes = toml::to_string(commit).map_err(|e| StoreError::Record(e.to_string()))?;
        self.write_immutable(&format!("commits/{commit_id}.toml"), commit_bytes.as_bytes())?;
        if !probe.at(Stage::SyncRecords) {
            return Err(StoreError::Io(io::Error::new(io::ErrorKind::Other, "probe abort: sync records")));
        }

        if let Some(v) = version {
            let vb = toml::to_string(v).map_err(|e| StoreError::Record(e.to_string()))?;
            self.write_immutable(&format!("versions/{}.toml", v.id.to_hex()), vb.as_bytes())?;
        }
        if let Some(c) = content {
            self.write_immutable(&format!("content/{}", SourceVersion::content_sha256(c)), c)?;
        }
        // Append to the published-record manifest — the durable list of every
        // commit id ever selected into state. Rebuildable (SQLite/cache can
        // be regenerated from it); reset never removes entries.
        {
            let mut m = fs::OpenOptions::new().append(true).open(self.root.join("published"))?;
            m.write_all(commit_id.as_bytes())?;
            m.write_all(b"\n")?;
        }

        if !probe.at(Stage::WriteTempState) {
            return Err(StoreError::Io(io::Error::new(io::ErrorKind::Other, "probe abort: write temp state")));
        }
        let state_bytes = toml::to_string(&new_state).map_err(|e| StoreError::Record(e.to_string()))?;
        let tmp = self.root.join("state.toml.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(state_bytes.as_bytes())?;
            f.sync_all()?;
        }
        if !probe.at(Stage::SyncTempState) {
            return Err(StoreError::Io(io::Error::new(io::ErrorKind::Other, "probe abort: sync temp state")));
        }

        if !probe.at(Stage::RenameState) {
            return Err(StoreError::Io(io::Error::new(io::ErrorKind::Other, "probe abort: rename state")));
        }
        fs::rename(&tmp, self.root.join("state.toml"))?;

        if !probe.at(Stage::SyncDir) {
            return Err(StoreError::Io(io::Error::new(io::ErrorKind::Other, "probe abort: dir sync")));
        }
        File::open(&self.root)?.sync_all()?;

        self.state = new_state;
        Ok(())
    }
}

/// The state visible to a single fixed-state read. Readers pin `state.toml`
/// once, then re-check participants' publication markers before reporting
/// success (E-3/E-4).
pub fn pin_state(root: &Path) -> Result<State, StoreError> {
    let s = fs::read_to_string(root.join("state.toml"))?;
    toml::from_str(&s).map_err(|e| StoreError::Record(e.to_string()))
}
