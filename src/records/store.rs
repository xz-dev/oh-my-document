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
use crate::records::registration::{ProjectRegistration, RemoteIdentity};
use crate::records::time::OsRng;
use crate::records::version::{Acquisition, SourceVersion};
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

/// Stable project identity. Shared aliases and remote constraints live in
/// immutable registration records selected by state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub format: String,
    pub project_id: String,
}

/// Identity exposed to discovery and local placement validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreIdentity {
    pub project_id: String,
    pub store_id: String,
    pub registrations: BTreeMap<String, String>,
    pub activated: bool,
}

/// On-disk mutable index. Everything readers need is reachable from this;
/// nothing else on disk is authoritative "current" state.
///
/// `format` stamps the schema generation — "omd.state/9" selects exact
/// planned-record inbound receipts, structured positions, and bidirectional
/// external endpoint records while physical placements stay machine-local.
/// store written without those semantics is refused, never reinterpreted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    /// Store format generation. ABSENT on disk = the old coordinate-key
    /// generation — serde fills `omd.state/1` for a missing field so the
    /// open-time check can refuse it. New stores are stamped by the
    /// constructor path with `omd.state/9`.
    #[serde(default = "legacy_state_format")]
    pub format: String,
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
    /// Object reference → current project-relative location. This is a
    /// derived projection of immutable init/rename history, never identity.
    /// Only file objects have entries; ranges reach location through mounts.
    #[serde(default)]
    pub locations: BTreeMap<String, String>,
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
    /// peer store_id → immutable logical registration record id.
    /// Physical paths live only in machine-local configuration.
    #[serde(default)]
    pub peers: BTreeMap<String, String>,
    /// protection credential id → immutable inbound record id. The selected
    /// revision is fixed by state; older pinned states keep reading the same
    /// immutable record.
    #[serde(default)]
    pub inbound: BTreeMap<String, String>,
}

/// What a missing `format` field means on disk: the coordinate-key
/// generation predates this field — deserialize it as the legacy marker so
/// open() can refuse it (never silently upgrade).
fn legacy_state_format() -> String {
    "omd.state/1".into()
}

/// The current store format generation. Bump when the on-disk meaning of
/// keys changes — a store written under an older generation is refused at
/// open, never migrated (spec: no dual-format reads, no silent upgrades).
fn current_state_format() -> String {
    "omd.state/9".into()
}

fn valid_identity(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Default for State {
    fn default() -> Self {
        Self {
            format: current_state_format(),
            publication: 0,
            operation_id: None,
            tips: Default::default(),
            mounts: Default::default(),
            locations: Default::default(),
            bindings: Default::default(),
            registrations: Default::default(),
            retained: Default::default(),
            dirty: Default::default(),
            open_blocks: Default::default(),
            reset_from: Default::default(),
            links: Default::default(),
            link_pending: Default::default(),
            tags: Default::default(),
            tag_rules: Default::default(),
            store_id: String::new(),
            activated: false,
            peers: Default::default(),
            inbound: Default::default(),
        }
    }
}

/// Immutable logical peer registration selected by state. Machine-local
/// paths and approvals never enter this shared record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerReg {
    pub format: String,
    pub peer_project_id: String,
    pub peer_store_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_id: Option<String>,
}

/// Immutable inbound protection credential. It names the consumer authority,
/// its selected logical registration, the exact unpublished/published record,
/// and the target chain root + historical version protected in this store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundCred {
    pub format: String,
    pub credential_id: String,
    pub consumer_project_id: String,
    pub consumer_store_id: String,
    pub consumer_registration_id: String,
    /// Exact planned/published consumer business commit identity.
    pub record_id: String,
    /// Independent immutable link identity carried by that commit.
    pub link_id: String,
    pub target_root: String,
    pub target_version: String,
}

/// Caller-observed peer state. Returned by verify/check and revalidated after
/// canonical ordered lock acquisition; mutation code never refreshes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerExpected {
    pub instance: String,
    pub mapping_revision: u64,
    pub project_id: String,
    pub store_id: String,
    pub publication: u64,
    pub tips: BTreeMap<String, String>,
    pub registrations: BTreeMap<String, String>,
    pub peers: BTreeMap<String, String>,
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
    /// Persistent endpoint objects (kind + chain root).
    pub source: String,
    pub target: String,
    /// Exact endpoint versions selected when this link was published.
    pub source_version: String,
    pub target_version: String,
    /// The commit that created it (identity separate from the link itself).
    pub created_by: String,
}

/// What a caller observed before writing — checked under the lock.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    /// Observation credential generation. Older implicit credentials are
    /// rejected rather than reinterpreted under newer evidence semantics.
    pub format: String,
    /// Selected physical metadata/project instance fingerprint.
    pub instance: String,
    /// Machine-local placement revision, when selection used a registered map.
    #[serde(default)]
    pub mapping_revision: Option<u64>,
    /// Logical identities protect against a copied/activated instance.
    pub project_id: String,
    pub store_id: String,
    /// Publication marker the caller last saw (must equal current).
    pub publication: u64,
    /// Relevant tips observed by verify/check (node key → commit id hex).
    pub tips: BTreeMap<String, String>,
    /// Stable source versions selected by the observed authoritative tips.
    /// These are compared with the locked store before fresh acquisitions are
    /// consumed; they are not evidence that acquisition itself succeeded.
    pub basis_versions: BTreeMap<String, String>,
    /// Successfully observed source versions reusable by a later write.
    pub source_versions: BTreeMap<String, String>,
    /// Successful acquisition records (node key → exact source version id).
    /// Equal hashes under a different source view are not interchangeable.
    pub acquisition_versions: BTreeMap<String, String>,
    /// Hashes of bytes from those successful acquisitions.
    pub source_hashes: BTreeMap<String, String>,
    /// Selected immutable logical peer registration ids.
    #[serde(default)]
    pub peer_registrations: BTreeMap<String, String>,
    /// Successfully observed registered peers. Offline unrelated peers are
    /// omitted; a mutation that needs one must require its entry explicitly.
    #[serde(default)]
    pub peers: BTreeMap<String, PeerExpected>,
    /// Specific registrations that must still hold.
    pub registrations: BTreeMap<String, String>,
}

impl Expected {
    pub fn observe(
        store: &Store,
        instance: String,
        mapping_revision: Option<u64>,
    ) -> Result<Self, StoreError> {
        let mut basis_versions = BTreeMap::new();
        let mut source_versions = BTreeMap::new();
        let mut acquisition_versions = BTreeMap::new();
        let mut source_hashes = BTreeMap::new();
        let identity_ok = store.identity_diagnostic().is_none();
        for node in store.state.tips.keys() {
            let Some(version_id) = store.source_version_id(node)? else {
                continue;
            };
            basis_versions.insert(node.clone(), version_id.clone());
            if !identity_ok {
                continue;
            }
            let Ok(version) = store.read_version(&version_id) else {
                continue;
            };
            let Some(acquisition) = store.current_acquisition(node, &version) else {
                continue;
            };
            if acquisition != version.acquisition {
                continue;
            }
            let sha256 = match &acquisition {
                crate::records::version::Acquisition::File { .. } => {
                    let Ok(path) = store.resolve_file_descriptor(&acquisition) else {
                        continue;
                    };
                    let Ok(bytes) = fs::read(path) else {
                        continue;
                    };
                    SourceVersion::content_sha256(&bytes)
                }
                crate::records::version::Acquisition::Command { .. } => continue,
                crate::records::version::Acquisition::Git { .. } => continue,
            };
            if sha256 != version.sha256 {
                continue;
            }
            source_versions.insert(node.clone(), version_id.clone());
            acquisition_versions.insert(node.clone(), version_id);
            source_hashes.insert(node.clone(), sha256);
        }
        Ok(Self {
            format: "omd.expected/4".into(),
            instance,
            mapping_revision,
            project_id: store.manifest.project_id.clone(),
            store_id: store.state.store_id.clone(),
            publication: store.state.publication,
            tips: store.state.tips.clone(),
            basis_versions,
            source_versions,
            acquisition_versions,
            source_hashes,
            peer_registrations: store.state.peers.clone(),
            peers: BTreeMap::new(),
            registrations: store.state.registrations.clone(),
        })
    }
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
    manifest: ProjectManifest,
    /// True only for the Store value that created a wholly absent metadata
    /// directory. Reopening always requires an exact machine-local authority
    /// mapping; copied directories never inherit this process-local grant.
    initialized_here: bool,
    /// Set only by locked cross-store validation for this Store value. It
    /// permits inbound protection metadata, never ordinary business writes.
    validated_peer_instance: bool,
    context_instance: String,
    mapping_revision: Option<u64>,
    project_root: Option<PathBuf>,
    metadata_root: PathBuf,
    context_alias: Option<String>,
    config_cwd: Option<PathBuf>,
    recognized_remote_urls: BTreeSet<String>,
    observed_versions: BTreeMap<String, SourceVersion>,
    observed_bytes: BTreeMap<String, Vec<u8>>,
    validated_expected: Option<String>,
}

impl Store {
    fn read_existing(root: &Path) -> Result<(State, ProjectManifest), StoreError> {
        let state_path = root.join("state.toml");
        let manifest_path = root.join("manifest.toml");
        if !state_path.is_file() {
            return Err(StoreError::Record(format!(
                "metadata identity incomplete at {} (state.toml required)",
                root.display()
            )));
        }
        let state_text = fs::read_to_string(&state_path)?;
        let state: State =
            toml::from_str(&state_text).map_err(|e| StoreError::Record(e.to_string()))?;
        if state.format != "omd.state/9" {
            return Err(StoreError::Record(format!(
                "unsupported store format '{}' — this build reads omd.state/9; the store is not migrated",
                state.format
            )));
        }
        if !manifest_path.is_file() {
            return Err(StoreError::Record(format!(
                "metadata identity incomplete at {} (manifest.toml required)",
                root.display()
            )));
        }
        let manifest_text = fs::read_to_string(&manifest_path)?;
        let manifest: ProjectManifest =
            toml::from_str(&manifest_text).map_err(|e| StoreError::Record(e.to_string()))?;
        if manifest.format != "omd.project/2" {
            return Err(StoreError::Record(format!(
                "unsupported project manifest format '{}' — this build reads omd.project/2; the store is not migrated",
                manifest.format
            )));
        }
        if !valid_identity(&manifest.project_id) || !valid_identity(&state.store_id) {
            return Err(StoreError::Record(
                "project/store identity must be 32 lowercase hexadecimal characters".into(),
            ));
        }
        if manifest.project_id == state.store_id {
            return Err(StoreError::Record(
                "project_id and store_id must be distinct identities".into(),
            ));
        }
        for required in [
            "commits",
            "versions",
            "content",
            "bindings",
            "registrations",
            "notes",
            "inbound",
            "pending",
        ] {
            if !root.join(required).is_dir() {
                return Err(StoreError::Record(format!(
                    "metadata directory missing required {required}/"
                )));
            }
        }
        if !root.join("published").is_file() {
            return Err(StoreError::Record(
                "metadata directory missing required published manifest".into(),
            ));
        }
        for (node, tip) in &state.tips {
            if !tip.is_empty() && !root.join(format!("commits/{tip}.toml")).is_file() {
                return Err(StoreError::Record(format!(
                    "tip for {node} references a missing commit record: {tip}"
                )));
            }
        }
        Ok((state, manifest))
    }

    /// Read identity without creating, repairing, or otherwise mutating the
    /// candidate metadata directory.
    pub fn identity_at(root: &Path) -> Result<StoreIdentity, StoreError> {
        let (state, manifest) = Self::read_existing(root)?;
        Ok(StoreIdentity {
            project_id: manifest.project_id,
            store_id: state.store_id,
            registrations: state.registrations,
            activated: state.activated,
        })
    }

    /// Open an existing store. Missing layout is an error; nothing is created.
    pub fn open_existing(root: &Path) -> Result<Self, StoreError> {
        let (state, manifest) = Self::read_existing(root)?;
        let store = Self {
            root: root.to_path_buf(),
            lock: None,
            state,
            manifest,
            initialized_here: false,
            validated_peer_instance: false,
            context_instance: String::new(),
            mapping_revision: None,
            project_root: None,
            metadata_root: root.to_path_buf(),
            context_alias: None,
            config_cwd: None,
            recognized_remote_urls: BTreeSet::new(),
            observed_versions: BTreeMap::new(),
            observed_bytes: BTreeMap::new(),
            validated_expected: None,
        };
        store.validate_identity_index()?;
        Ok(store)
    }

    /// Open an existing store or initialize a wholly absent metadata dir.
    /// CLI callers use this creating path only for explicit project init.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        Self::open_new(root, false)
    }

    /// Initialize a metadata directory containing only a pre-validated
    /// `omd.toml`, preserving that file byte-for-byte.
    pub fn open_configured(root: &Path) -> Result<Self, StoreError> {
        Self::open_new(root, true)
    }

    fn open_new(root: &Path, allow_configuration: bool) -> Result<Self, StoreError> {
        if root.join("state.toml").exists() || root.join("manifest.toml").exists() {
            if allow_configuration {
                return Err(StoreError::Record(format!(
                    "configured initialization requires only omd.toml: {}",
                    root.display()
                )));
            }
            return Self::open_existing(root);
        }
        if allow_configuration && !root.is_dir() {
            return Err(StoreError::Record(format!(
                "configured initialization requires only omd.toml: {}",
                root.display()
            )));
        }
        if root.exists() {
            let entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
            let configuration_only = allow_configuration
                && entries.len() == 1
                && entries[0].file_name() == "omd.toml"
                && entries[0].path().is_file();
            if (allow_configuration && !configuration_only)
                || (!allow_configuration && !entries.is_empty())
            {
                return Err(StoreError::Record(format!(
                    "metadata directory is not empty and has no supported identity: {}",
                    root.display()
                )));
            }
        }
        for d in [
            "commits",
            "versions",
            "content",
            "bindings",
            "registrations",
            "notes",
            "inbound",
            "pending",
        ] {
            fs::create_dir_all(root.join(d))?;
        }
        let store_id = crate::records::cross::new_store_id(&OsRng);
        let state = State {
            store_id: store_id.clone(),
            activated: true,
            ..State::default()
        };
        let mut project_id = crate::records::cross::new_store_id(&OsRng);
        while project_id == store_id {
            project_id = crate::records::cross::new_store_id(&OsRng);
        }
        let manifest = ProjectManifest {
            format: "omd.project/2".into(),
            project_id,
        };
        fs::write(
            root.join("state.toml"),
            toml::to_string(&state).map_err(|e| StoreError::Record(e.to_string()))?,
        )?;
        fs::write(
            root.join("manifest.toml"),
            toml::to_string(&manifest).map_err(|e| StoreError::Record(e.to_string()))?,
        )?;
        fs::write(root.join("published"), "")?;
        let store = Self {
            root: root.to_path_buf(),
            lock: None,
            state,
            manifest,
            initialized_here: true,
            validated_peer_instance: false,
            context_instance: String::new(),
            mapping_revision: None,
            project_root: None,
            metadata_root: root.to_path_buf(),
            context_alias: None,
            config_cwd: None,
            recognized_remote_urls: BTreeSet::new(),
            observed_versions: BTreeMap::new(),
            observed_bytes: BTreeMap::new(),
            validated_expected: None,
        };
        store.validate_identity_index()?;
        Ok(store)
    }

    pub fn identity(&self) -> StoreIdentity {
        StoreIdentity {
            project_id: self.manifest.project_id.clone(),
            store_id: self.state.store_id.clone(),
            registrations: self.state.registrations.clone(),
            activated: self.state.activated,
        }
    }

    /// Publish a new immutable shared project registration revision.
    pub fn register_project(
        &mut self,
        alias: &str,
        remote: Option<RemoteIdentity>,
        expected: &Expected,
    ) -> Result<String, StoreError> {
        if alias.is_empty() || alias == "root" {
            return Err(StoreError::Record(
                "project alias must be non-empty and cannot be reserved alias 'root'".into(),
            ));
        }
        if let Some(remote) = &remote
            && (remote.name.is_empty()
                || remote.url.is_empty()
                || remote.name.contains(['\n', '\r', '\0'])
                || remote.url.contains(['\n', '\r', '\0']))
        {
            return Err(StoreError::Record(
                "remote name and URL must be non-empty single-line values".into(),
            ));
        }
        self.lock()?;
        self.check_metadata_expected(expected)?;
        let key = format!("project:{alias}");
        let current = self
            .state
            .registrations
            .get(&key)
            .map(|_| self.project_registration(alias))
            .transpose()?;
        if current
            .as_ref()
            .is_some_and(|registration| registration.remote == remote)
        {
            return Ok(self.state.registrations[&key].clone());
        }
        let record = ProjectRegistration::new(
            alias.to_string(),
            self.manifest.project_id.clone(),
            self.state.store_id.clone(),
            current
                .as_ref()
                .map_or(1, |registration| registration.revision + 1),
            remote,
        );
        let id = record
            .id()
            .map_err(|error| StoreError::Record(error.to_string()))?;
        let bytes =
            toml::to_string(&record).map_err(|error| StoreError::Record(error.to_string()))?;
        self.write_immutable(&format!("registrations/{id}.toml"), bytes.as_bytes())?;
        let mut state = self.state.clone();
        state.registrations.insert(key, id.clone());
        state.publication += 1;
        self.set_state_locked(state)?;
        Ok(id)
    }

    pub fn project_registration(&self, alias: &str) -> Result<ProjectRegistration, StoreError> {
        let id = self
            .state
            .registrations
            .get(&format!("project:{alias}"))
            .ok_or_else(|| {
                StoreError::Record(format!("project alias is not registered: {alias}"))
            })?;
        let text = fs::read_to_string(self.root.join(format!("registrations/{id}.toml")))?;
        let registration: ProjectRegistration =
            toml::from_str(&text).map_err(|error| StoreError::Record(error.to_string()))?;
        if registration.format != "omd.registration/1"
            || registration.alias != alias
            || registration.project_id != self.manifest.project_id
            || registration.store_id != self.state.store_id
            || registration.revision == 0
            || registration
                .id()
                .map_err(|error| StoreError::Record(error.to_string()))?
                != *id
        {
            return Err(StoreError::Record(format!(
                "project registration {alias} is invalid"
            )));
        }
        Ok(registration)
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
        let (state, manifest) = Self::read_existing(&self.root)?;
        self.state = state;
        self.manifest = manifest;
        self.observed_versions.clear();
        self.observed_bytes.clear();
        self.validated_expected = None;
        self.validated_peer_instance = false;
        self.lock = Some(WriteLock { _file: lf });
        Ok(())
    }

    pub fn is_locked(&self) -> bool {
        self.lock.is_some()
    }

    pub fn bind_context(
        &mut self,
        instance: String,
        mapping_revision: Option<u64>,
        project_root: PathBuf,
        context_alias: Option<String>,
        config_cwd: PathBuf,
        recognized_remote_urls: BTreeSet<String>,
    ) {
        self.context_instance = instance;
        self.mapping_revision = mapping_revision;
        self.project_root = Some(project_root);
        self.context_alias = context_alias;
        self.config_cwd = Some(config_cwd);
        self.recognized_remote_urls = recognized_remote_urls;
    }

    /// Validate exact selected mapping and raw configured Git remote.
    /// No configured remote means no Git process is spawned.
    pub fn identity_diagnostic(&self) -> Option<String> {
        let alias = match self.context_alias.as_deref() {
            Some(alias) => alias,
            None => {
                if self
                    .state
                    .registrations
                    .keys()
                    .any(|key| key.starts_with("project:"))
                {
                    return Some(
                        "registered project identity has no selected local mapping; select or repair it explicitly"
                            .into(),
                    );
                }
                return None;
            }
        };
        let cwd = self.config_cwd.as_deref()?;
        let project_root = self.project_root.as_deref()?;
        let maps = match crate::sources::projects::load(cwd) {
            Ok(maps) => maps,
            Err(error) => return Some(format!("project mapping {alias} unreadable: {error}")),
        };
        let matches: Vec<_> = maps
            .iter()
            .filter(|map| {
                map.alias == alias
                    && map.project_id == self.manifest.project_id
                    && map.store_id == self.state.store_id
                    && map.project_root == project_root
                    && map.metadata_root == self.metadata_root
            })
            .collect();
        let Some(mapping) = matches.first().copied() else {
            return Some(format!(
                "project mapping {alias} is missing; run `omd project list` or register it explicitly"
            ));
        };
        if matches.iter().any(|candidate| **candidate != *mapping) {
            return Some(format!(
                "project mapping {alias} is ambiguous; run `omd project list` or register it explicitly"
            ));
        }
        if Some(mapping.revision) != self.mapping_revision
            || mapping.recognized_remote_urls != self.recognized_remote_urls
        {
            return Some(format!(
                "project mapping {alias} revision changed; rerun verify/check"
            ));
        }
        let registration = match self.project_registration(alias) {
            Ok(registration) => registration,
            Err(error) => {
                return Some(format!(
                    "project registration {alias} unreadable: {error}; inspect with `omd project list` and correct with `omd project register`"
                ));
            }
        };
        let remote = registration.remote?;
        let key = format!("remote.{}.url", remote.name);
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(project_root)
            .args(["config", "--null", "--get-all", &key])
            .output();
        let output = match output {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                return Some(format!(
                    "project {alias} remote {} is absent or unreadable (git exit {}); correct registration or local mapping",
                    remote.name,
                    output
                        .status
                        .code()
                        .map_or_else(|| "signal".into(), |code| code.to_string())
                ));
            }
            Err(error) => {
                return Some(format!(
                    "project {alias} remote {} is unreadable: {error}; correct registration or local mapping",
                    remote.name
                ));
            }
        };
        let Some(value) = output.stdout.strip_suffix(&[0]) else {
            return Some(format!(
                "project {alias} remote {} has malformed configured URL framing",
                remote.name
            ));
        };
        if value.contains(&0) {
            return Some(format!(
                "project {alias} remote {} has ambiguous configured URLs",
                remote.name
            ));
        }
        if value.is_empty() {
            return Some(format!(
                "project {alias} remote {} has malformed configured URL",
                remote.name
            ));
        }
        let actual = match std::str::from_utf8(value) {
            Ok(actual) => actual,
            Err(_) => {
                return Some(format!(
                    "project {alias} remote {} has non-UTF-8 configured URL",
                    remote.name
                ));
            }
        };
        let registration_id = self.state.registrations.get(&format!("project:{alias}"));
        let locally_recognized = mapping.recognized_registration.as_ref() == registration_id;
        if actual != remote.url
            && !(locally_recognized && mapping.recognized_remote_urls.contains(actual))
        {
            return Some(format!(
                "project {alias} remote {} URL mismatch: registered {:?}, actual {:?}; correct with `omd project register` or explicitly recognize this local URL",
                remote.name, remote.url, actual
            ));
        }
        None
    }

    pub fn require_identity(&self) -> Result<(), StoreError> {
        self.identity_diagnostic()
            .map_or(Ok(()), |diagnostic| Err(StoreError::Conflict(diagnostic)))
    }

    /// Business publication requires this exact physical instance to be the
    /// machine-local authority. A newly created Store value may publish its
    /// first records; reopening always requires an authority mapping.
    pub fn has_write_authority(&self) -> bool {
        if !self.state.activated {
            return false;
        }
        if self.initialized_here {
            return true;
        }
        let (Some(cwd), Some(project_root)) =
            (self.config_cwd.as_deref(), self.project_root.as_deref())
        else {
            return false;
        };
        crate::sources::projects::authority_matches(
            cwd,
            project_root,
            &self.metadata_root,
            &self.identity(),
        )
        .unwrap_or(false)
    }

    pub fn require_write_authority(&self) -> Result<(), StoreError> {
        self.require_identity()?;
        if self.has_write_authority() {
            Ok(())
        } else {
            Err(StoreError::Conflict(
                "store is an unregistered copy: business writes require explicit activation".into(),
            ))
        }
    }

    pub(crate) fn mark_validated_peer_instance(&mut self) {
        self.validated_peer_instance = true;
    }

    pub(crate) fn require_inbound_authority(&self) -> Result<(), StoreError> {
        if self.validated_peer_instance || self.has_write_authority() {
            Ok(())
        } else {
            Err(StoreError::Conflict(
                "inbound protection requires an exact validated peer instance".into(),
            ))
        }
    }

    pub fn observed_bytes(&self, node: &str) -> Option<&[u8]> {
        self.observed_bytes.get(node).map(Vec::as_slice)
    }

    pub fn observed_version(&self, node: &str) -> Option<(&SourceVersion, &[u8])> {
        Some((
            self.observed_versions.get(node)?,
            self.observed_bytes.get(node)?.as_slice(),
        ))
    }

    /// Persist one successful source acquisition without publishing a
    /// business commit. Equal acquisition/view/content reuses its version.
    pub fn persist_observation(
        &mut self,
        version: &SourceVersion,
        bytes: &[u8],
    ) -> Result<SourceVersion, StoreError> {
        version.validate().map_err(StoreError::Record)?;
        self.lock()?;
        if let Ok(entries) = fs::read_dir(self.root.join("versions")) {
            for entry in entries.flatten() {
                if let Ok(text) = fs::read_to_string(entry.path())
                    && let Ok(existing) = toml::from_str::<SourceVersion>(&text)
                    && existing.reuse_key() == version.reuse_key()
                    && existing.encoding == version.encoding
                {
                    return Ok(existing);
                }
            }
        }
        let mut stored = version.clone();
        stored.content_file = Some(format!("content/{}", stored.sha256));
        let text = toml::to_string(&stored).map_err(|e| StoreError::Record(e.to_string()))?;
        self.write_immutable(
            &format!("versions/{}.toml", stored.id.to_hex()),
            text.as_bytes(),
        )?;
        self.write_immutable(&format!("content/{}", stored.sha256), bytes)?;
        Ok(stored)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Read a commit record by id (immutable `commits/<id>.toml`).
    pub fn read_commit(&self, id: &str) -> Result<crate::records::commit::Commit, StoreError> {
        let s = fs::read_to_string(self.root.join(format!("commits/{id}.toml")))?;
        let commit: crate::records::commit::Commit =
            toml::from_str(&s).map_err(|e| StoreError::Record(e.to_string()))?;
        commit
            .validate(commit.previous_id.is_empty())
            .map_err(|e| StoreError::Record(format!("commit {id}: {e}")))?;
        Ok(commit)
    }

    /// Validate every mutable object projection against immutable ancestry.
    /// A changed key/path index can make a store unreadable, never reinterpret
    /// a commit as a different object or silently manufacture a chain root.
    fn validate_identity_index(&self) -> Result<(), StoreError> {
        for key in self.state.registrations.keys() {
            if let Some(alias) = key.strip_prefix("project:") {
                self.project_registration(alias)?;
            }
        }
        let mut seen_locations = BTreeMap::<String, String>::new();
        let mut projected_ranges = BTreeSet::new();
        for children in self.state.mounts.values() {
            for child in children {
                if !projected_ranges.insert(child.clone()) {
                    return Err(StoreError::Record(format!(
                        "range object is mounted more than once: {child}"
                    )));
                }
            }
        }
        for (node, tip) in &self.state.tips {
            let (kind, indexed_root) = node
                .split_once(':')
                .ok_or_else(|| StoreError::Record(format!("invalid object key in tips: {node}")))?;
            if !matches!(kind, "file" | "range") || indexed_root.is_empty() {
                return Err(StoreError::Record(format!(
                    "invalid object key in tips: {node}"
                )));
            }
            let authoritative = crate::relations::identity::authoritative_object(self, tip)
                .map_err(|error| StoreError::Record(format!("{node}: {error}")))?;
            if authoritative.root_commit_id != indexed_root {
                return Err(StoreError::Record(format!(
                    "object index mismatch: {node} points to chain rooted at {}",
                    authoritative.root_commit_id
                )));
            }
            let expected_kind = crate::relations::identity::kind_name(authoritative.kind);
            if kind != expected_kind {
                return Err(StoreError::Record(format!(
                    "object kind mismatch: {node} projects {kind}, immutable chain is {expected_kind}"
                )));
            }
            if kind == "file" {
                if projected_ranges.contains(node) {
                    return Err(StoreError::Record(format!(
                        "file object is mounted as a range: {node}"
                    )));
                }
                match authoritative.location {
                    Some(location) => {
                        if self.state.locations.get(node) != Some(&location) {
                            return Err(StoreError::Record(format!(
                                "location index mismatch for {node}: expected {location:?}, found {:?}",
                                self.state.locations.get(node)
                            )));
                        }
                        if let Some(other) = seen_locations.insert(location.clone(), node.clone()) {
                            return Err(StoreError::Record(format!(
                                "duplicate current location {location:?} for {other} and {node}"
                            )));
                        }
                    }
                    None if self.state.locations.contains_key(node) => {
                        return Err(StoreError::Record(format!(
                            "tombstoned file has active location entry: {node}"
                        )));
                    }
                    None => {}
                }
            } else {
                if self.state.locations.contains_key(node) {
                    return Err(StoreError::Record(format!(
                        "range object has file location entry: {node}"
                    )));
                }
                let parent = crate::relations::node::parent_of(&self.state, node);
                if parent != authoritative.parent.as_deref() {
                    return Err(StoreError::Record(format!(
                        "range mount mismatch for {node}: expected {:?}, found {parent:?}",
                        authoritative.parent
                    )));
                }
                if !projected_ranges.contains(node) {
                    return Err(StoreError::Record(format!(
                        "range object is not mounted: {node}"
                    )));
                }
            }
        }
        for node in self.state.locations.keys() {
            if !self.state.tips.contains_key(node) || !node.starts_with("file:") {
                return Err(StoreError::Record(format!(
                    "orphan location index entry: {node}"
                )));
            }
        }
        for (parent, children) in &self.state.mounts {
            if !self.state.tips.contains_key(parent) || !parent.starts_with("file:") {
                return Err(StoreError::Record(format!(
                    "range mount parent is not a live file object: {parent}"
                )));
            }
            for child in children {
                if !self.state.tips.contains_key(child) || !child.starts_with("range:") {
                    return Err(StoreError::Record(format!(
                        "range mount child is not a live range object: {child}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn source_path<'a>(&'a self, node: &str) -> Option<&'a str> {
        self.state
            .locations
            .get(node)
            .map(String::as_str)
            .or_else(|| {
                crate::relations::node::parent_of(&self.state, node)
                    .and_then(|parent| self.state.locations.get(parent))
                    .map(String::as_str)
            })
    }

    /// Resolve current observation independently from historical recovery.
    /// File-backed objects follow owning file object's authoritative current
    /// location; command/Git definitions remain exact.
    pub fn current_acquisition(&self, node: &str, version: &SourceVersion) -> Option<Acquisition> {
        match &version.acquisition {
            Acquisition::File { .. } => Some(Acquisition::File {
                project: "root".into(),
                path: self.source_path(node)?.to_string(),
            }),
            other => Some(other.clone()),
        }
    }

    pub fn source_version_id(&self, node: &str) -> Result<Option<String>, StoreError> {
        let mut current = self.state.tips.get(node).cloned().unwrap_or_default();
        let mut guard = 0usize;
        while !current.is_empty() && guard < 100_000 {
            let commit = self.read_commit(&current)?;
            if commit.content_ref != "empty" {
                return Ok(Some(commit.content_ref));
            }
            current = commit.previous_id;
            guard += 1;
        }
        Ok(None)
    }

    pub fn command_observation(
        &self,
        expected: &Expected,
        node: &str,
        executable: &str,
        args: &[String],
    ) -> Result<Vec<u8>, StoreError> {
        self.require_source_expected(expected, node)?;
        let version_id = expected.acquisition_versions.get(node).ok_or_else(|| {
            StoreError::Conflict(format!(
                "successful command observation for {node} is required"
            ))
        })?;
        let version = self.read_version(version_id)?;
        let wanted = crate::records::version::Acquisition::Command {
            executable: executable.to_string(),
            args: args.to_vec(),
        };
        if version.acquisition != wanted {
            return Err(StoreError::Conflict(format!(
                "command acquisition for {node} changed"
            )));
        }
        self.observed_bytes.get(node).cloned().ok_or_else(|| {
            StoreError::Conflict(format!(
                "successful command observation for {node} is required"
            ))
        })
    }

    pub fn require_source_expected(
        &self,
        expected: &Expected,
        node: &str,
    ) -> Result<(), StoreError> {
        if self.source_version_id(node)?.is_some()
            && (!expected.basis_versions.contains_key(node)
                || !expected.source_versions.contains_key(node)
                || !expected.acquisition_versions.contains_key(node)
                || !expected.source_hashes.contains_key(node)
                || !self.observed_bytes.contains_key(node))
        {
            return Err(StoreError::Conflict(format!(
                "successful source observation for {node} is required"
            )));
        }
        Ok(())
    }

    /// Read a version record by id (`versions/<id>.toml`).
    pub fn read_version(
        &self,
        id: &str,
    ) -> Result<crate::records::version::SourceVersion, StoreError> {
        let s = fs::read_to_string(self.root.join(format!("versions/{id}.toml")))?;
        let version: crate::records::version::SourceVersion =
            toml::from_str(&s).map_err(|e| StoreError::Record(e.to_string()))?;
        version.validate().map_err(StoreError::Record)?;
        if version.id.to_hex() != id {
            return Err(StoreError::Record(format!(
                "source version identity mismatch: expected {id}, found {}",
                version.id.to_hex()
            )));
        }
        Ok(version)
    }

    pub fn read_binding(
        &self,
        version_id: &str,
    ) -> Result<Option<crate::records::binding::Binding>, StoreError> {
        let Some(id) = self.state.bindings.get(version_id) else {
            return Ok(None);
        };
        let text = fs::read_to_string(self.root.join(format!("bindings/{id}.toml")))?;
        let binding: crate::records::binding::Binding =
            toml::from_str(&text).map_err(|error| StoreError::Record(error.to_string()))?;
        binding
            .validate(id, version_id)
            .map_err(StoreError::Record)?;
        Ok(Some(binding))
    }

    pub fn recovery_descriptor(
        &self,
        version: &SourceVersion,
    ) -> Result<crate::sources::SourceDescriptor, StoreError> {
        Ok(self
            .read_binding(&version.id.to_hex())?
            .map(|binding| binding.recovery)
            .unwrap_or_else(|| version.recovery.clone()))
    }

    pub fn verify_exact_git_recovery(&self, version: &SourceVersion) -> Result<bool, StoreError> {
        let recovery = self.recovery_descriptor(version)?;
        if !matches!(recovery, crate::sources::SourceDescriptor::Git { .. }) {
            return Ok(false);
        }
        let cwd = self.config_cwd.as_deref().ok_or_else(|| {
            StoreError::Conflict("source recovery requires machine-local configuration".into())
        })?;
        let project_root = self.project_root.as_deref().ok_or_else(|| {
            StoreError::Conflict("source recovery requires selected project root".into())
        })?;
        let Ok(observation) = crate::sources::collect(&recovery, cwd, project_root, false, None)
        else {
            return Ok(false);
        };
        Ok(observation.bytes.len() as u64 == version.len
            && SourceVersion::content_sha256(&observation.bytes) == version.sha256)
    }

    /// Read exact complete historical bytes without executing commands.
    /// Local immutable content wins. Missing local content is recoverable only
    /// through an exact Git descriptor whose bytes still match this version.
    pub fn recover_version_bytes(&self, version_id: &str) -> Result<Vec<u8>, StoreError> {
        let version = self.read_version(version_id)?;
        if let Some(path) = &version.content_file {
            match fs::read(self.root.join(path)) {
                Ok(bytes) => {
                    if bytes.len() as u64 != version.len
                        || SourceVersion::content_sha256(&bytes) != version.sha256
                    {
                        return Err(StoreError::Record(format!(
                            "source content for {version_id} does not match its complete version"
                        )));
                    }
                    return Ok(bytes);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(StoreError::Io(error)),
            }
        }
        let recovery = self.recovery_descriptor(&version)?;
        if !matches!(recovery, crate::sources::SourceDescriptor::Git { .. }) {
            return Err(StoreError::Record(format!(
                "source version {version_id} has no non-executing complete recovery"
            )));
        }
        let cwd = self.config_cwd.as_deref().ok_or_else(|| {
            StoreError::Conflict("source recovery requires machine-local configuration".into())
        })?;
        let project_root = self.project_root.as_deref().ok_or_else(|| {
            StoreError::Conflict("source recovery requires selected project root".into())
        })?;
        let observation = crate::sources::collect(&recovery, cwd, project_root, false, None)
            .map_err(|error| StoreError::Record(error.to_string()))?;
        if observation.bytes.len() as u64 != version.len
            || SourceVersion::content_sha256(&observation.bytes) != version.sha256
        {
            return Err(StoreError::Record(format!(
                "recovered source for {version_id} differs from recorded complete content"
            )));
        }
        Ok(observation.bytes)
    }

    pub fn resolve_file_descriptor(
        &self,
        descriptor: &crate::sources::SourceDescriptor,
    ) -> Result<PathBuf, StoreError> {
        let crate::sources::SourceDescriptor::File { project, path } = descriptor else {
            return Err(StoreError::Record("source is not a file descriptor".into()));
        };
        let cwd = self.config_cwd.as_deref().ok_or_else(|| {
            StoreError::Conflict("source observation requires machine-local configuration".into())
        })?;
        let root = self.project_root.as_deref().ok_or_else(|| {
            StoreError::Conflict("source observation requires selected project root".into())
        })?;
        crate::sources::projects::resolve(cwd, root, project, path)
            .map_err(|error| StoreError::Conflict(error.to_string()))
    }

    /// Read stored content bytes by sha256 (`content/<sha256>`).
    pub fn read_content(&self, sha256: &str) -> Result<Vec<u8>, StoreError> {
        Ok(fs::read(self.root.join(format!("content/{sha256}")))?)
    }

    /// The store root path (for resolving `commits/`, `versions/`, `content/`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub fn metadata_root(&self) -> &Path {
        &self.metadata_root
    }

    pub fn config_cwd(&self) -> Option<&Path> {
        self.config_cwd.as_deref()
    }

    pub fn context_instance(&self) -> &str {
        &self.context_instance
    }

    pub(crate) fn write_record(&self, rel: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.write_immutable(rel, bytes)
    }

    pub(crate) fn set_state_locked_public(&mut self, state: State) -> Result<(), StoreError> {
        self.set_state_locked(state)
    }

    /// Publish one immutable recovery binding after caller evidence is checked
    /// under the writer lock. No business commit or original version changes.
    pub fn publish_binding(
        &mut self,
        expected: &Expected,
        binding: &crate::records::binding::Binding,
    ) -> Result<(), StoreError> {
        self.lock()?;
        self.require_write_authority()?;
        self.check_expected(expected)?;
        binding
            .validate(&binding.id, &binding.version_id)
            .map_err(StoreError::Record)?;
        let bytes =
            toml::to_string(binding).map_err(|error| StoreError::Record(error.to_string()))?;
        self.write_immutable(&format!("bindings/{}.toml", binding.id), bytes.as_bytes())?;
        let mut state = self.state.clone();
        state
            .bindings
            .insert(binding.version_id.clone(), binding.id.clone());
        state.publication += 1;
        self.set_state_locked(state)
    }

    /// Persist a metadata-only state update through the shared precondition
    /// boundary. Validation occurs after lock acquisition.
    pub fn set_state_expected(&mut self, expected: &Expected, st: State) -> Result<(), StoreError> {
        self.lock()?;
        self.require_write_authority()?;
        self.check_expected(expected)?;
        self.set_state_locked(st)
    }

    /// Complete the narrow local state transition for an explicitly prepared
    /// writable copy. Business state is preserved byte-for-byte; only store
    /// identity, project registration selection, activation, and publication
    /// advance. Required peer protection is prepared by the activation flow
    /// before this final publication.
    pub fn publish_copy_activation(
        &mut self,
        expected: &Expected,
        new_store_id: &str,
        registrations: BTreeMap<String, String>,
    ) -> Result<(), StoreError> {
        self.lock()?;
        if expected.format != "omd.expected/4"
            || expected.instance != self.context_instance
            || expected.mapping_revision != self.mapping_revision
            || expected.project_id != self.manifest.project_id
            || expected.store_id != self.state.store_id
            || expected.publication != self.state.publication
            || expected.tips != self.state.tips
            || expected.registrations != self.state.registrations
            || expected.peer_registrations != self.state.peers
        {
            return Err(StoreError::Conflict(
                "copy activation evidence changed before final publication".into(),
            ));
        }
        if self.has_write_authority() {
            return Err(StoreError::Conflict(
                "selected instance is already writable".into(),
            ));
        }
        if !valid_identity(new_store_id)
            || new_store_id == self.state.store_id
            || new_store_id == self.manifest.project_id
        {
            return Err(StoreError::Record(
                "activated copy requires a new distinct 128-bit store identity".into(),
            ));
        }
        if registrations.keys().collect::<BTreeSet<_>>()
            != self.state.registrations.keys().collect::<BTreeSet<_>>()
        {
            return Err(StoreError::Record(
                "copy activation cannot add or remove project registrations".into(),
            ));
        }
        for (key, selected_id) in &registrations {
            let Some(alias) = key.strip_prefix("project:") else {
                if self.state.registrations.get(key) != Some(selected_id) {
                    return Err(StoreError::Record(
                        "copy activation changed a non-project registration".into(),
                    ));
                }
                continue;
            };
            let text =
                fs::read_to_string(self.root.join(format!("registrations/{selected_id}.toml")))?;
            let registration: ProjectRegistration =
                toml::from_str(&text).map_err(|error| StoreError::Record(error.to_string()))?;
            if registration.alias != alias
                || registration.project_id != self.manifest.project_id
                || registration.store_id != new_store_id
                || registration
                    .id()
                    .map_err(|error| StoreError::Record(error.to_string()))?
                    != *selected_id
            {
                return Err(StoreError::Record(format!(
                    "copy activation registration is inconsistent: {alias}"
                )));
            }
        }
        let cwd = self.config_cwd.as_deref().ok_or_else(|| {
            StoreError::Conflict("copy activation requires local configuration".into())
        })?;
        let project_root = self.project_root.as_deref().ok_or_else(|| {
            StoreError::Conflict("copy activation requires a selected project root".into())
        })?;
        let new_identity = StoreIdentity {
            project_id: self.manifest.project_id.clone(),
            store_id: new_store_id.to_string(),
            registrations: registrations.clone(),
            activated: true,
        };
        if !crate::sources::projects::authority_matches(
            cwd,
            project_root,
            &self.metadata_root,
            &new_identity,
        )
        .map_err(|error| StoreError::Conflict(error.to_string()))?
        {
            return Err(StoreError::Conflict(
                "activated copy authority mapping is missing or mismatched".into(),
            ));
        }
        let mut state = self.state.clone();
        state.store_id = new_store_id.to_string();
        state.registrations = registrations;
        state.activated = true;
        state.publication += 1;
        self.set_state_locked(state)
    }

    fn set_state_locked(&mut self, st: State) -> Result<(), StoreError> {
        let path = self.root.join("state.toml");
        let tmp = self.root.join("state.toml.tmp");
        let txt = toml::to_string(&st).map_err(|e| StoreError::Record(e.to_string()))?;
        {
            let mut f = File::create(&tmp)?;
            f.write_all(txt.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        File::open(&self.root)?.sync_all()?;
        self.state = st;
        Ok(())
    }

    /// Validate metadata-only caller evidence under the writer lock. This
    /// intentionally skips remote/source success so an identity mismatch can
    /// be repaired, but still pins the exact physical target and authority.
    pub fn check_metadata_expected(&mut self, exp: &Expected) -> Result<(), StoreError> {
        self.lock()?;
        self.validate_metadata_expected(exp)
    }

    fn validate_metadata_expected(&self, exp: &Expected) -> Result<(), StoreError> {
        if exp.format != "omd.expected/4" {
            return Err(StoreError::Conflict(format!(
                "unsupported expected evidence format '{}'",
                exp.format
            )));
        }
        if exp.instance != self.context_instance {
            return Err(StoreError::Conflict(
                "selected project/store instance changed".into(),
            ));
        }
        let live_mapping_revision = match (
            self.config_cwd.as_deref(),
            self.project_root.as_deref(),
            self.context_alias.as_deref(),
        ) {
            (Some(cwd), Some(_), Some(alias)) => {
                let maps = crate::sources::projects::load(cwd).map_err(|error| {
                    StoreError::Conflict(format!("project mapping {alias} unreadable: {error}"))
                })?;
                let matching: Vec<_> = maps
                    .iter()
                    .filter(|map| {
                        map.alias == alias
                            && map.project_id == self.manifest.project_id
                            && map.store_id == self.state.store_id
                    })
                    .collect();
                let [mapping] = matching.as_slice() else {
                    return Err(StoreError::Conflict(if matching.is_empty() {
                        format!("project mapping {alias} is missing")
                    } else {
                        format!("project mapping {alias} is ambiguous")
                    }));
                };
                // A project directory may remain after the authority moves.
                // Reject another live metadata placement, not a live code root.
                if mapping.metadata_root != self.metadata_root && mapping.metadata_root.exists() {
                    return Err(StoreError::Conflict(format!(
                        "project mapping {alias} points to a different live location"
                    )));
                }
                Some(mapping.revision)
            }
            (Some(_), Some(_), None) if self.mapping_revision.is_none() => None,
            _ => {
                return Err(StoreError::Conflict(
                    "project mapping context is incomplete".into(),
                ));
            }
        };
        if exp.mapping_revision != live_mapping_revision {
            return Err(StoreError::Conflict(
                "project mapping revision changed".into(),
            ));
        }
        if exp.project_id != self.manifest.project_id || exp.store_id != self.state.store_id {
            return Err(StoreError::Conflict(
                "project/store identity changed".into(),
            ));
        }
        if exp.publication != self.state.publication {
            return Err(StoreError::Conflict(format!(
                "publication {} != current {}",
                exp.publication, self.state.publication
            )));
        }
        if exp.tips != self.state.tips {
            return Err(StoreError::Conflict(
                "tip evidence is incomplete or changed".into(),
            ));
        }
        if exp.registrations != self.state.registrations {
            return Err(StoreError::Conflict(
                "registration evidence is incomplete or changed".into(),
            ));
        }
        if exp.peer_registrations != self.state.peers {
            return Err(StoreError::Conflict(
                "peer registration evidence is incomplete or changed".into(),
            ));
        }
        Ok(())
    }

    /// Verify caller-observed preconditions under the held lock.
    /// Any mismatch aborts the write — never silently uses the newer value.
    pub fn check_expected(&mut self, exp: &Expected) -> Result<(), StoreError> {
        self.lock()?;
        self.require_identity()?;
        let fingerprint =
            serde_json::to_string(exp).map_err(|error| StoreError::Record(error.to_string()))?;
        if self.validated_expected.as_deref() == Some(fingerprint.as_str()) {
            return Ok(());
        }
        self.validate_metadata_expected(exp)?;
        let acquired_nodes: BTreeSet<&String> = exp.acquisition_versions.keys().collect();
        if acquired_nodes != exp.source_versions.keys().collect()
            || acquired_nodes != exp.source_hashes.keys().collect()
        {
            return Err(StoreError::Conflict(
                "successful source evidence is incomplete".into(),
            ));
        }
        self.observed_versions.clear();
        self.observed_bytes.clear();
        for (node, observed_version_id) in &exp.source_versions {
            let basis_version_id = exp
                .basis_versions
                .get(node)
                .ok_or_else(|| StoreError::Conflict(format!("source basis for {node} missing")))?;
            if self.source_version_id(node)?.as_deref() != Some(basis_version_id.as_str()) {
                return Err(StoreError::Conflict(format!(
                    "source basis for {node} changed"
                )));
            }
            let acquisition_version_id = exp.acquisition_versions.get(node).ok_or_else(|| {
                StoreError::Conflict(format!("acquisition version for {node} missing"))
            })?;
            if acquisition_version_id != observed_version_id {
                return Err(StoreError::Conflict(format!(
                    "source/acquisition version for {node} differs"
                )));
            }
            let basis_version = self.read_version(basis_version_id)?;
            let observed_version = self.read_version(observed_version_id)?;
            if basis_version.id.to_hex() != *basis_version_id
                || observed_version.id.to_hex() != *observed_version_id
            {
                return Err(StoreError::Conflict(format!(
                    "source version record for {node} has inconsistent identity"
                )));
            }
            let current_acquisition =
                self.current_acquisition(node, &basis_version)
                    .ok_or_else(|| {
                        StoreError::Conflict(format!("current source for {node} is unavailable"))
                    })?;
            if current_acquisition != observed_version.acquisition {
                return Err(StoreError::Conflict(format!(
                    "source observation definition for {node} changed: expected {current_acquisition:?}, observed {:?}",
                    observed_version.acquisition
                )));
            }
            self.recover_version_bytes(basis_version_id)
                .map_err(|error| {
                    StoreError::Conflict(format!("source basis for {node} is invalid: {error}"))
                })?;
            let observed_hash = exp
                .source_hashes
                .get(node)
                .ok_or_else(|| StoreError::Conflict(format!("source hash for {node} missing")))?;
            let bytes = match &observed_version.acquisition {
                crate::records::version::Acquisition::File { .. } => {
                    let path = self.resolve_file_descriptor(&observed_version.acquisition)?;
                    fs::read(path)?
                }
                _ => self.read_content(observed_hash)?,
            };
            if &observed_version.sha256 != observed_hash
                || SourceVersion::content_sha256(&bytes) != *observed_hash
            {
                return Err(StoreError::Conflict(format!(
                    "source observation tuple for {node} is inconsistent"
                )));
            }
            self.observed_versions
                .insert(node.clone(), observed_version);
            self.observed_bytes.insert(node.clone(), bytes);
        }
        self.validated_expected = Some(fingerprint);
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
            // Content is content-addressed; reused observation versions are
            // immutable too and may be selected by a later business commit.
            if rel.starts_with("content/")
                || (rel.starts_with("versions/") && fs::read(&p)? == bytes)
            {
                return Ok(());
            }
            return Err(StoreError::Record(format!(
                "immutable record exists: {rel}"
            )));
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
        self.require_write_authority()?;
        if let Some(version) = version {
            version.validate().map_err(StoreError::Record)?;
        }
        if !probe.at(Stage::WriteRecords) {
            return Err(StoreError::Io(io::Error::other(
                "probe abort: write records",
            )));
        }
        let commit_bytes =
            toml::to_string(commit).map_err(|e| StoreError::Record(e.to_string()))?;
        self.write_immutable(
            &format!("commits/{commit_id}.toml"),
            commit_bytes.as_bytes(),
        )?;
        if !probe.at(Stage::SyncRecords) {
            return Err(StoreError::Io(io::Error::other(
                "probe abort: sync records",
            )));
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
            let mut m = fs::OpenOptions::new()
                .append(true)
                .open(self.root.join("published"))?;
            m.write_all(commit_id.as_bytes())?;
            m.write_all(b"\n")?;
        }

        if !probe.at(Stage::WriteTempState) {
            return Err(StoreError::Io(io::Error::other(
                "probe abort: write temp state",
            )));
        }
        let state_bytes =
            toml::to_string(&new_state).map_err(|e| StoreError::Record(e.to_string()))?;
        let tmp = self.root.join("state.toml.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(state_bytes.as_bytes())?;
            f.sync_all()?;
        }
        if !probe.at(Stage::SyncTempState) {
            return Err(StoreError::Io(io::Error::other(
                "probe abort: sync temp state",
            )));
        }

        if !probe.at(Stage::RenameState) {
            return Err(StoreError::Io(io::Error::other(
                "probe abort: rename state",
            )));
        }
        fs::rename(&tmp, self.root.join("state.toml"))?;

        if !probe.at(Stage::SyncDir) {
            return Err(StoreError::Io(io::Error::other("probe abort: dir sync")));
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
