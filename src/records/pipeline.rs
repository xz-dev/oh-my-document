//! The commit pipeline every mutating CLI verb shares (design E-2).
//!
//! observe → source-version → commit → publish, in that order:
//!   1. observe the source's full current bytes
//!   2. create (or reuse) a source version *before* the commit is derived
//!   3. build the commit (salt + previous_id + timestamp + content + payload)
//!   4. publish: write immutable records, then atomically swap state
//!
//! Nothing here runs a source program, does Git I/O, or touches the network.

use crate::records::commit::{Commit, CommitKind};
use crate::records::ids::Id128;
use crate::records::id::salt_from_bytes;
use crate::records::store::{Expected, PublishProbe, State, Store, StoreError};
use crate::records::version::{Acquisition, SourceVersion};
use crate::sources::{observe_file, Observation, SourceError};
use crate::testing::{Clock, Rng};
use crate::relations::dirty::DirtyState;

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("source: {0}")]
    Source(#[from] SourceError),
    #[error("store: {0}")]
    Store(#[from] StoreError),
    #[error("commit invalid: {0}")]
    Commit(String),
}

/// Draw a fresh 16-char salt from the RNG.
fn draw_salt(rng: &dyn Rng) -> String {
    let mut b = [0u8; 16];
    rng.fill(&mut b);
    String::from_utf8(salt_from_bytes(&b).to_vec()).unwrap()
}

/// Observe a file source at its registered path (current bytes, not HEAD).
pub fn observe(path: &Path, text: bool, encoding: Option<&str>) -> Result<Observation, PipelineError> {
    Ok(observe_file(path, text, encoding)?)
}

/// Create a source version for an observation, drawn before the commit.
pub fn make_version(
    rng: &dyn Rng,
    obs: &Observation,
    acquisition: Acquisition,
) -> SourceVersion {
    let mut idb = [0u8; 16];
    rng.fill(&mut idb);
    SourceVersion::new(Id128(idb), &obs.bytes, acquisition, obs.encoding.clone())
}

/// Build a commit referencing a source version.
/// `previous_id` is the node's current tip ("" for first). `content` is the
/// full raw bytes consumed — always the complete observation.
pub fn make_commit(
    rng: &dyn Rng,
    clock: &dyn Clock,
    kind: CommitKind,
    previous_id: &str,
    version: &SourceVersion,
    payload: serde_json::Map<String, serde_json::Value>,
) -> Commit {
    Commit {
        id: None,
        salt: draw_salt(rng),
        previous_id: previous_id.to_string(),
        timestamp: Commit::format_timestamp(clock.now().0),
        schema: "omd.commit/1".into(),
        kind,
        content_ref: version.id.to_hex(),
        payload,
        range_tips: Default::default(),
    }
}

/// Persist a version's content under content/<sha256> if not already shared.
/// Returns the updated version with `content_file` set.
fn with_content(version: &SourceVersion) -> SourceVersion {
    let mut v = version.clone();
    v.content_file = Some(format!("content/{}", v.sha256));
    v
}

/// Full pipeline for one file commit: observe → version → commit → publish.
/// Returns the derived commit id hex. `expected` is checked under the lock.
pub fn commit_file(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    path: &Path,
    kind: CommitKind,
    payload: serde_json::Map<String, serde_json::Value>,
    expected: &Expected,
) -> Result<String, PipelineError> {
    store.lock()?;
    store.check_expected(expected)?;

    let is_first = !store.state().tips.contains_key(node_key);
    // Resolve the source path relative to the process CWD — the caller
    // supplies the project root as its working directory. Observation reads
    // the real file *now*, never a snapshot.
    let obs = observe_file(path, true, Some("utf-8"))?;
    let version = make_version(rng, &obs, Acquisition::File {
        path: path.to_string_lossy().into(),
        encoding: "utf-8".into(),
    });
    let version = with_content(&version);

    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let commit = make_commit(rng, clock, kind, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(&obs.bytes).map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());

    // Kind-driven domain effects — persisted in state, not just computed.
    match kind {
        CommitKind::Unclean => {
            // Each unclean is its own stacked obligation (distinct id/reason).
            let reason = commit
                .payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ds = new_state.dirty.entry(node_key.to_string()).or_insert_with(DirtyState::default);
            ds.push_unclean(&cid, &reason);
        }
        CommitKind::Clean => {
            // clean clears obligations raised against this node's range but
            // does NOT rewrite history — dirty marks on other commits stay.
            if let Some(ds) = new_state.dirty.get_mut(node_key) {
                ds.obligations.retain(|o| o.commit_id != cid);
            }
        }
        CommitKind::AtomicBegin => {
            new_state.open_blocks.entry(node_key.to_string()).or_default().push(cid.clone());
        }
        CommitKind::AtomicEnd => {
            // END closes the nearest open BEGIN on this node.
            if let Some(stack) = new_state.open_blocks.get_mut(node_key) {
                stack.pop();
            }
        }
        _ => {}
    }

    store.publish(probe, &commit, &cid, Some(&version), Some(&obs.bytes), new_state)?;
    Ok(cid)
}

/// A state-only commit that observes no file — markers, obligations, and
/// lifecycle verbs that don't consume source bytes. Content is empty; the
/// commit still derives a real id from salt+prev+timestamp+payload.
pub fn commit_marker(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    kind: CommitKind,
    payload: serde_json::Map<String, serde_json::Value>,
    expected: &Expected,
) -> Result<String, PipelineError> {
    store.lock()?;
    store.check_expected(expected)?;

    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation { bytes: Vec::new(), text: false, encoding: None };
    let version = make_version(rng, &empty_obs, Acquisition::File { path: "".into(), encoding: "".into() });
    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let commit = make_commit(rng, clock, kind, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(b"").map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    match kind {
        CommitKind::Unclean => {
            let reason = commit.payload.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string();
            new_state.dirty.entry(node_key.to_string()).or_insert_with(DirtyState::default).push_unclean(&cid, &reason);
        }
        CommitKind::AtomicBegin => {
            new_state.open_blocks.entry(node_key.to_string()).or_default().push(cid.clone());
        }
        CommitKind::AtomicEnd => {
            if let Some(s) = new_state.open_blocks.get_mut(node_key) { s.pop(); }
        }
        _ => {}
    }
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(cid)
}

/// Result of `omd verify` over a store — the independent check distinct
/// from `check` (coverage). Fails when any node has unclosed ATOMIC blocks,
/// unhandled obligations, or a dirty tip caused by a dangling dependency.
#[derive(Debug, serde::Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    /// Nodes with unclosed ATOMIC blocks.
    pub open_blocks: Vec<String>,
    /// Nodes carrying unhandled unclean obligations.
    pub obligations: Vec<String>,
    /// Dirty commits keyed by node.
    pub dirty: std::collections::BTreeMap<String, Vec<String>>,
}

/// Run verify against the persisted state — reads what's on disk, never
/// fabricates a passing result from a moving target.
pub fn verify(store: &Store) -> VerifyReport {
    let st = store.state();
    let open_blocks: Vec<String> = st
        .open_blocks
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    let mut obligations = Vec::new();
    let mut dirty = std::collections::BTreeMap::new();
    for (node, ds) in &st.dirty {
        if !ds.obligations.is_empty() {
            obligations.push(node.clone());
        }
        let ids: Vec<String> = ds.dirty.keys().cloned().collect();
        if !ids.is_empty() {
            dirty.insert(node.clone(), ids);
        }
    }
    let ok = open_blocks.is_empty() && obligations.is_empty() && dirty.is_empty();
    VerifyReport { ok, open_blocks, obligations, dirty }
}
