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
use crate::records::id::salt_from_bytes;
use crate::records::ids::Id128;
use crate::records::store::{Expected, PeerExpected, PublishProbe, State, Store, StoreError};
use crate::records::version::{Acquisition, SourceVersion};
use crate::sources::{Observation, SourceError, observe_file};
use crate::testing::{Clock, Rng};

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("source: {0}")]
    Source(#[from] SourceError),
    #[error("store: {0}")]
    Store(#[from] StoreError),
    #[error("invalid input: {0}")]
    Input(String),
    #[error("commit invalid: {0}")]
    Commit(String),
}

fn validate_range_payload(
    payload: &serde_json::Map<String, serde_json::Value>,
    bytes: &[u8],
    encoding: Option<&str>,
) -> Result<(), PipelineError> {
    let Some(range) = payload
        .get("position")
        .and_then(crate::relations::node::position_from_value)
    else {
        return Ok(());
    };
    let len = match range.mode {
        crate::relations::range::Mode::Byte => bytes.len() as u64,
        crate::relations::range::Mode::Text => crate::relations::range::text_len(
            &crate::sources::decode(bytes, encoding.unwrap_or("utf-8"))
                .map_err(|_| PipelineError::Input("range source cannot be decoded".into()))?,
        ),
    };
    crate::relations::range::Range::new(range.start, range.end, range.mode, len)
        .map(|_| ())
        .map_err(|e| PipelineError::Input(e.to_string()))
}

/// Draw a fresh 16-char salt from the RNG.
fn draw_salt(rng: &dyn Rng) -> String {
    let mut b = [0u8; 16];
    rng.fill(&mut b);
    String::from_utf8(salt_from_bytes(&b).to_vec()).unwrap()
}

/// Resolve an internal node locator to the authoritative object key. Public
/// state stores only chain-root keys; the path form remains accepted here for
/// existing library callers while tests and CLI migrate to explicit lookups.
fn object_key(store: &Store, locator: &str) -> String {
    if store.state().tips.contains_key(locator) {
        return locator.to_string();
    }
    locator
        .strip_prefix("file:")
        .and_then(|path| crate::relations::node::file_at_path(store.state(), path))
        .unwrap_or(locator)
        .to_string()
}

fn node_source_version(store: &Store, node: &str) -> Option<SourceVersion> {
    let mut current = store.state().tips.get(node)?.clone();
    let mut guard = 0usize;
    while !current.is_empty() && guard < 100_000 {
        let commit = store.read_commit(&current).ok()?;
        if commit.content_ref != "empty" {
            return store.read_version(&commit.content_ref).ok();
        }
        current = commit.previous_id;
        guard += 1;
    }
    None
}

fn canonicalize_mount(store: &Store, payload: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(parent) = payload
        .get("mount")
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return;
    };
    let parent = object_key(store, &parent);
    payload.insert("mount".into(), parent.into());
}

fn published_node_key(locator: &str, is_first: bool, commit_id: &str) -> String {
    if is_first && crate::relations::node::is_file_key(locator) {
        crate::relations::node::file_key(commit_id)
    } else if is_first && locator == "range:pending" {
        crate::relations::node::range_key(commit_id)
    } else {
        locator.to_string()
    }
}

/// Observe a file source at its registered path (current bytes, not HEAD).
pub fn observe(
    path: &Path,
    text: bool,
    encoding: Option<&str>,
) -> Result<Observation, PipelineError> {
    Ok(observe_file(path, text, encoding)?)
}

/// Create a source version for an observation, drawn before the commit.
pub fn make_version(rng: &dyn Rng, obs: &Observation, acquisition: Acquisition) -> SourceVersion {
    make_version_with_recovery(rng, obs, acquisition.clone(), acquisition)
}

pub fn make_version_with_recovery(
    rng: &dyn Rng,
    obs: &Observation,
    acquisition: Acquisition,
    recovery: Acquisition,
) -> SourceVersion {
    let mut idb = [0u8; 16];
    rng.fill(&mut idb);
    SourceVersion::new_with_recovery(
        Id128(idb),
        &obs.bytes,
        acquisition,
        recovery,
        obs.encoding.clone(),
    )
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
        schema: "omd.commit/3".into(),

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

fn normalize_persisted_source(
    store: &Store,
    descriptor: Acquisition,
) -> Result<Acquisition, PipelineError> {
    match (store.config_cwd(), store.project_root()) {
        (Some(cwd), Some(root)) => Ok(crate::sources::normalize_descriptor(descriptor, cwd, root)?),
        _ => {
            descriptor.validate_portable()?;
            Ok(descriptor)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn commit_source(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    acquisition: Acquisition,
    recovery: Acquisition,
    initial_observation: Option<Observation>,
    encoding_override: Option<&str>,
    kind: CommitKind,
    mut payload: serde_json::Map<String, serde_json::Value>,
    expected: &Expected,
) -> Result<String, PipelineError> {
    let acquisition = normalize_persisted_source(store, acquisition)?;
    let recovery = normalize_persisted_source(store, recovery)?;
    store.lock()?;
    store.require_write_authority()?;

    let node_key = object_key(store, node_key);
    canonicalize_mount(store, &mut payload);
    let is_first = !store.state().tips.contains_key(&node_key);
    let unobserved_init = kind == CommitKind::Init && is_first;
    if !unobserved_init {
        store.check_expected(expected)?;
        let evidence_node = if expected.source_versions.contains_key(&node_key) {
            &node_key
        } else {
            payload
                .get("mount")
                .and_then(|value| value.as_str())
                .unwrap_or(&node_key)
        };
        store.require_source_expected(expected, evidence_node)?;
    }
    let byte_mode = payload
        .get("position")
        .and_then(crate::relations::node::position_from_value)
        .is_some_and(|range| range.mode == crate::relations::range::Mode::Byte);
    let (obs, observed_version) = if unobserved_init {
        let mut observation = initial_observation.ok_or_else(|| {
            PipelineError::Input("new source requires one collected observation".into())
        })?;
        if byte_mode {
            observation.text = false;
            observation.encoding = None;
        }
        (observation, None)
    } else {
        let evidence_node = if expected.source_versions.contains_key(&node_key) {
            &node_key
        } else {
            payload
                .get("mount")
                .and_then(|value| value.as_str())
                .unwrap_or(&node_key)
        };
        let (version, bytes) = store.observed_version(evidence_node).ok_or_else(|| {
            PipelineError::Store(StoreError::Conflict(format!(
                "successful source observation for {evidence_node} is required"
            )))
        })?;
        if version.acquisition != acquisition {
            return Err(PipelineError::Store(StoreError::Conflict(format!(
                "source observation definition for {evidence_node} changed"
            ))));
        }
        let observed_is_byte = version.encoding.is_none();
        let mut observation = Observation {
            bytes: bytes.to_vec(),
            text: !observed_is_byte,
            encoding: version.encoding.clone(),
        };
        let view_changed = !byte_mode
            && !observed_is_byte
            && encoding_override
                .is_some_and(|encoding| version.encoding.as_deref() != Some(encoding));
        if view_changed {
            let encoding = encoding_override.expect("changed view has override");
            crate::sources::decode(&observation.bytes, encoding)?;
            observation.text = true;
            observation.encoding = Some(encoding.to_string());
        }
        (observation, (!view_changed).then(|| version.clone()))
    };
    validate_range_payload(&payload, &obs.bytes, obs.encoding.as_deref())?;
    let version = match observed_version {
        Some(version) => version,
        None => {
            let version = make_version_with_recovery(rng, &obs, acquisition, recovery);
            if matches!(version.recovery, Acquisition::Git { .. }) {
                version
            } else {
                with_content(&version)
            }
        }
    };

    let prev = store
        .state()
        .tips
        .get(&node_key)
        .cloned()
        .unwrap_or_default();
    let mut commit = make_commit(rng, clock, kind, &prev, &version, payload);
    if crate::relations::node::is_range_key(&node_key) {
        let parent = crate::relations::node::parent_of(store.state(), &node_key)
            .map(str::to_string)
            .or_else(|| {
                commit
                    .payload
                    .get("mount")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        if let Some(parent) = parent
            && let Some(begin_id) = store
                .state()
                .open_blocks
                .get(&parent)
                .and_then(|b| b.last())
        {
            commit
                .payload
                .insert("in_block".into(), begin_id.clone().into());
        }
    }
    if crate::relations::node::is_file_key(&node_key)
        && !is_first
        && let Some(children) = store.state().mounts.get(&node_key)
    {
        for child in children {
            if let Some(tip) = store.state().tips.get(child) {
                commit.range_tips.insert(child.clone(), tip.clone());
            }
        }
    }
    commit
        .validate(is_first)
        .map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit
        .derive_id(&obs.bytes)
        .map_err(|e| PipelineError::Commit(e.to_string()))?
        .to_hex();
    let published_key = published_node_key(&node_key, is_first, &cid);

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(published_key.clone(), cid.clone());
    new_state.retained.push(cid.clone());
    if is_first && crate::relations::node::is_file_key(&node_key) {
        let location = commit
            .payload
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| PipelineError::Input("new file object requires path".into()))?;
        if crate::relations::node::file_at_path(&new_state, location).is_some() {
            return Err(PipelineError::Input(format!(
                "location is already tracked: {location}"
            )));
        }
        new_state
            .locations
            .insert(published_key.clone(), location.into());
    }
    if crate::relations::node::is_range_key(&published_key) && is_first {
        let parent = commit
            .payload
            .get("mount")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PipelineError::Input("new range requires file object mount".into()))?;
        if !new_state.tips.contains_key(parent) || !crate::relations::node::is_file_key(parent) {
            return Err(PipelineError::Input(format!(
                "range mount is not a live file object: {parent}"
            )));
        }
        new_state
            .mounts
            .entry(parent.into())
            .or_default()
            .push(published_key.clone());
    }

    let direct: Vec<String> = new_state
        .links
        .iter()
        .filter(|(_, link)| link.source == published_key)
        .map(|(id, _)| id.clone())
        .collect();
    for link_id in &direct {
        new_state
            .link_pending
            .entry(link_id.clone())
            .or_default()
            .insert(cid.clone());
    }
    let downstream_sources: Vec<String> = direct
        .iter()
        .filter_map(|id| new_state.links.get(id).map(|link| link.target.clone()))
        .collect();
    let transitive: Vec<String> = new_state
        .links
        .iter()
        .filter(|(_, link)| downstream_sources.contains(&link.source))
        .map(|(id, _)| id.clone())
        .collect();
    for link_id in transitive {
        new_state
            .link_pending
            .entry(link_id)
            .or_default()
            .insert(cid.clone());
    }

    match kind {
        CommitKind::Unclean => {
            let reason = commit
                .payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            new_state
                .dirty
                .entry(published_key.clone())
                .or_default()
                .push_unclean(&cid, &reason);
        }
        CommitKind::Clean => {
            if let Some(dirty) = new_state.dirty.get_mut(&published_key) {
                dirty.obligations.retain(|o| o.commit_id != cid);
            }
        }
        CommitKind::AtomicBegin => new_state
            .open_blocks
            .entry(published_key.clone())
            .or_default()
            .push(cid.clone()),
        CommitKind::AtomicEnd => {
            if let Some(stack) = new_state.open_blocks.get_mut(&published_key) {
                stack.pop();
            }
        }
        _ => {}
    }

    let content = version.content_file.as_ref().map(|_| obs.bytes.as_slice());
    store.publish(probe, &commit, &cid, Some(&version), content, new_state)?;
    Ok(cid)
}

/// Compatibility library entry for direct file callers. CLI uses
/// `commit_source` so aliased file, command, and Git sources share one path.
#[allow(clippy::too_many_arguments)]
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
    encoding: Option<&str>,
) -> Result<String, PipelineError> {
    let node_key = object_key(store, node_key);
    let is_first = !store.state().tips.contains_key(&node_key);
    let descriptor = if is_first {
        let source_path = payload
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| PipelineError::Input("new file source requires path".into()))?
            .to_string();
        Acquisition::File {
            project: "root".into(),
            path: source_path,
        }
    } else {
        let version = node_source_version(store, &node_key).ok_or_else(|| {
            PipelineError::Input(format!("source version missing for {node_key}"))
        })?;
        store
            .current_acquisition(&node_key, &version)
            .ok_or_else(|| PipelineError::Input(format!("current source missing for {node_key}")))?
    };
    let initial = if kind == CommitKind::Init && is_first {
        let byte_mode = payload
            .get("position")
            .and_then(crate::relations::node::position_from_value)
            .is_some_and(|range| range.mode == crate::relations::range::Mode::Byte);
        Some(observe_file(
            path,
            !byte_mode,
            (!byte_mode).then_some(encoding.unwrap_or("utf-8")),
        )?)
    } else {
        None
    };
    commit_source(
        store,
        probe,
        rng,
        clock,
        &node_key,
        descriptor.clone(),
        descriptor,
        initial,
        encoding,
        kind,
        payload,
        expected,
    )
}

/// A state-only commit that observes no file — markers, obligations, and
/// lifecycle verbs that don't consume source bytes. Content is empty; the
/// commit still derives a real id from salt+prev+timestamp+payload.
#[allow(clippy::too_many_arguments)]
pub fn commit_marker(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    kind: CommitKind,
    mut payload: serde_json::Map<String, serde_json::Value>,
    expected: &Expected,
) -> Result<String, PipelineError> {
    store.lock()?;
    store.require_write_authority()?;

    let node_key = object_key(store, node_key);
    canonicalize_mount(store, &mut payload);
    let is_first = !store.state().tips.contains_key(&node_key);
    if !(kind == CommitKind::Init && is_first) {
        store.check_expected(expected)?;
    }
    let empty_obs = Observation {
        bytes: Vec::new(),
        text: false,
        encoding: None,
    };
    let version = make_version(
        rng,
        &empty_obs,
        Acquisition::File {
            project: "root".into(),
            path: "".into(),
        },
    );
    let prev = store
        .state()
        .tips
        .get(&node_key)
        .cloned()
        .unwrap_or_default();
    let mut commit = make_commit(rng, clock, kind, &prev, &version, payload);
    commit.content_ref = "empty".into();
    if crate::relations::node::is_file_key(&node_key)
        && !is_first
        && let Some(children) = store.state().mounts.get(&node_key)
    {
        for child in children {
            if let Some(tip) = store.state().tips.get(child) {
                commit.range_tips.insert(child.clone(), tip.clone());
            }
        }
    }
    commit
        .validate(is_first)
        .map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit
        .derive_id(b"")
        .map_err(|e| PipelineError::Commit(e.to_string()))?
        .to_hex();
    let published_key = published_node_key(&node_key, is_first, &cid);

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(published_key.clone(), cid.clone());
    new_state.retained.push(cid.clone());
    if is_first && crate::relations::node::is_file_key(&node_key) {
        let location = commit
            .payload
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| PipelineError::Input("new file object requires path".into()))?;
        if crate::relations::node::file_at_path(&new_state, location).is_some() {
            return Err(PipelineError::Input(format!(
                "location is already tracked: {location}"
            )));
        }
        new_state
            .locations
            .insert(published_key.clone(), location.into());
    }
    if crate::relations::node::is_range_key(&published_key) && is_first {
        let parent = commit
            .payload
            .get("mount")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PipelineError::Input("new range requires file object mount".into()))?;
        if !new_state.tips.contains_key(parent) || !crate::relations::node::is_file_key(parent) {
            return Err(PipelineError::Input(format!(
                "range mount is not a live file object: {parent}"
            )));
        }
        new_state
            .mounts
            .entry(parent.into())
            .or_default()
            .push(published_key.clone());
    }
    match kind {
        CommitKind::Unclean => {
            let reason = commit
                .payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            new_state
                .dirty
                .entry(published_key.clone())
                .or_default()
                .push_unclean(&cid, &reason);
        }
        CommitKind::AtomicBegin => new_state
            .open_blocks
            .entry(published_key.clone())
            .or_default()
            .push(cid.clone()),
        CommitKind::AtomicEnd => {
            if let Some(stack) = new_state.open_blocks.get_mut(&published_key) {
                stack.pop();
            }
        }
        CommitKind::Tag => {
            if let Some(tag) = commit.payload.get("tag").and_then(|v| v.as_str()) {
                new_state
                    .tags
                    .entry(published_key.clone())
                    .or_default()
                    .insert(tag.to_string());
            }
        }
        CommitKind::ScopeAdjust => {
            if let Some(rule) = commit.payload.get("rule").and_then(|v| v.as_str()) {
                let level = commit
                    .payload
                    .get("level")
                    .and_then(|v| v.as_str())
                    .unwrap_or("fail")
                    .to_string();
                let skip = commit
                    .payload
                    .get("skip")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                new_state.tag_rules.insert(
                    rule.to_string(),
                    crate::records::store::TagRule {
                        rule: rule.to_string(),
                        level,
                        skip,
                    },
                );
            }
        }
        _ => {}
    }
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(cid)
}

/// Lifecycle verbs that move node identity, not just append a record.
///
/// - `rename`: a commit on the source node recording `{path:{source,target}}`;
///   the tip + mounts + dirty state + ranges migrate to `file:<target>` — the
///   identity continues, the path changes. No Myers diff runs.
/// - `delete`: a tombstone on `file:<source>` (`target` null); history and
///   notes keep, dependents report `broken`. A vanished file with no tombstone
///   is `missing` — never auto-interpreted as a delete.
/// - `copy`: a fresh `init` on `file:<target>` — new identity, no copied
///   association. The source object is untouched.
#[allow(clippy::too_many_arguments)]
pub fn commit_lifecycle(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    kind: CommitKind,
    source_path: &str,
    target_path: Option<&str>,
    reason: &str,
    expected: &Expected,
) -> Result<String, PipelineError> {
    store.lock()?;
    store.require_write_authority()?;
    store.check_expected(expected)?;

    let src_key = crate::relations::node::file_at_path(store.state(), source_path)
        .ok_or_else(|| PipelineError::Input(format!("file is not tracked: {source_path}")))?
        .to_string();
    if kind == CommitKind::Rename {
        let target = target_path
            .ok_or_else(|| PipelineError::Input("rename requires target path".into()))?;
        if crate::relations::node::file_at_path(store.state(), target).is_some() {
            return Err(PipelineError::Input(format!(
                "rename target is already tracked: {target}"
            )));
        }
    }
    let empty_obs = Observation {
        bytes: Vec::new(),
        text: false,
        encoding: None,
    };
    let version = make_version(
        rng,
        &empty_obs,
        Acquisition::File {
            project: "root".into(),
            path: source_path.into(),
        },
    );

    let mut payload = serde_json::Map::new();
    payload.insert("path".into(), source_path.into());
    payload.insert("source".into(), source_path.into());
    if let Some(target) = target_path {
        payload.insert("target".into(), target.into());
    }
    payload.insert("reason".into(), reason.into());

    let prev = store.state().tips[&src_key].clone();
    let mut commit = make_commit(rng, clock, kind, &prev, &version, payload);
    commit.content_ref = "empty".into();
    commit
        .validate(false)
        .map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit
        .derive_id(b"")
        .map_err(|e| PipelineError::Commit(e.to_string()))?
        .to_hex();

    let mut new_state = store.state().clone();
    new_state.publication += 1;
    new_state.retained.push(cid.clone());
    new_state.tips.insert(src_key.clone(), cid.clone());
    if kind == CommitKind::Rename {
        new_state.locations.insert(
            src_key.clone(),
            target_path.expect("validated rename target").to_string(),
        );
    } else if kind == CommitKind::Delete {
        new_state.locations.remove(&src_key);
    }
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(cid)
}

/// Create a link instance between two range nodes. Endpoints are node
/// keys (`range:<chain-root>`) — the committing node itself may serve as
/// one endpoint. Refuses file-level linking. Each link gets a fresh
/// 128-bit id; identical endpoints+direction coexist as distinct instances.
#[allow(clippy::too_many_arguments)]
pub fn commit_link(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    source: &str,
    source_version: &str,
    target: &str,
    target_version: &str,
    reason: &str,
    expected: &Expected,
) -> Result<String, PipelineError> {
    // Local link creation accepts local range nodes only. Cross-store
    // endpoints must pass through commit_xlink, which validates store scope
    // and target existence before publication.
    if !crate::relations::node::is_range_key(source)
        || !crate::relations::node::is_range_key(target)
    {
        return Err(PipelineError::Commit(
            "local links connect local ranges; use a cross-store link entry for peer endpoints"
                .into(),
        ));
    }
    store.lock()?;
    store.require_write_authority()?;
    store.check_expected(expected)?;
    // Both endpoints must be real local range objects. Peer endpoints cannot
    // bypass CLI preflight through this lower-level local-link API.
    if !store.state().tips.contains_key(source) {
        return Err(PipelineError::Commit(format!(
            "link source range does not exist: {source}"
        )));
    }
    if !store.state().tips.contains_key(target) {
        return Err(PipelineError::Commit(format!(
            "link target range does not exist: {target}"
        )));
    }
    crate::relations::identity::resolve_version_on_chain(store, source, source_version)
        .map_err(|e| PipelineError::Commit(format!("link source version: {e}")))?;
    crate::relations::identity::resolve_version_on_chain(store, target, target_version)
        .map_err(|e| PipelineError::Commit(format!("link target version: {e}")))?;

    let mut idb = [0u8; 16];
    rng.fill(&mut idb);
    let link_id = crate::records::ids::Id128(idb).to_hex();

    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation {
        bytes: Vec::new(),
        text: false,
        encoding: None,
    };
    let version = make_version(
        rng,
        &empty_obs,
        Acquisition::File {
            project: "root".into(),
            path: "".into(),
        },
    );
    let prev = store
        .state()
        .tips
        .get(node_key)
        .cloned()
        .unwrap_or_default();
    let mut payload = serde_json::Map::new();
    payload.insert("link_id".into(), link_id.clone().into());
    payload.insert("source".into(), source.into());
    payload.insert("source_version".into(), source_version.into());
    payload.insert("target".into(), target.into());
    payload.insert("target_version".into(), target_version.into());
    payload.insert("reason".into(), reason.into());
    let commit = make_commit(rng, clock, CommitKind::Link, &prev, &version, payload);
    // Structural link commits consume no source bytes — content_ref is
    // "empty"; a phantom version id would point at a record never written.
    let commit = {
        let mut c = commit;
        c.content_ref = "empty".into();
        c
    };
    commit
        .validate(is_first)
        .map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit
        .derive_id(b"")
        .map_err(|e| PipelineError::Commit(e.to_string()))?
        .to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    new_state.links.insert(
        link_id.clone(),
        crate::records::store::Link {
            link_id: link_id.clone(),
            source: source.to_string(),
            target: target.to_string(),
            source_version: source_version.to_string(),
            target_version: target_version.to_string(),
            created_by: cid.clone(),
        },
    );
    new_state.link_pending.entry(link_id.clone()).or_default();
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(link_id)
}

/// Result of the cross-store protection-before-publication protocol.
#[derive(Debug, Clone)]
pub struct ProtectedXlink {
    pub link_id: String,
    pub credential_id: String,
}

#[derive(Debug)]
pub struct ProtectedXlinkFailure {
    pub error: PipelineError,
    pub link_id: String,
    pub credential_id: Option<String>,
}

struct PlannedXlink {
    link_id: String,
    record_id: String,
    commit: Commit,
    state: State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalLinkDirection {
    PeerToLocal,
    LocalToPeer,
}

#[allow(clippy::too_many_arguments)]
pub fn commit_xlink_protected(
    store: &mut Store,
    peer: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    source: &str,
    source_version: &str,
    peer_store_id: &str,
    peer_target_key: &str,
    peer_target_version: &str,
    link_id: &str,
    reason: &str,
    expected: &Expected,
    peer_expected: &PeerExpected,
) -> Result<ProtectedXlink, ProtectedXlinkFailure> {
    commit_external_link_protected(
        store,
        peer,
        probe,
        rng,
        clock,
        node_key,
        source,
        source_version,
        peer_store_id,
        peer_target_key,
        peer_target_version,
        link_id,
        reason,
        expected,
        peer_expected,
        ExternalLinkDirection::LocalToPeer,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn commit_external_link_protected(
    store: &mut Store,
    peer: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    local_endpoint: &str,
    local_version: &str,
    peer_store_id: &str,
    peer_endpoint: &str,
    peer_version: &str,
    link_id: &str,
    reason: &str,
    expected: &Expected,
    peer_expected: &PeerExpected,
    direction: ExternalLinkDirection,
) -> Result<ProtectedXlink, ProtectedXlinkFailure> {
    let fail = |error, credential_id| ProtectedXlinkFailure {
        error,
        link_id: link_id.to_string(),
        credential_id,
    };
    if !store.is_locked() && !peer.is_locked() {
        let local = std::fs::canonicalize(store.root())
            .map_err(StoreError::Io)
            .map_err(|error| fail(error.into(), None))?;
        let remote = std::fs::canonicalize(peer.root())
            .map_err(StoreError::Io)
            .map_err(|error| fail(error.into(), None))?;
        if local == remote {
            return Err(fail(
                PipelineError::Input("cross-store participants are the same store".into()),
                None,
            ));
        }
        if local < remote {
            store.lock().map_err(|error| fail(error.into(), None))?;
            peer.lock().map_err(|error| fail(error.into(), None))?;
        } else {
            peer.lock().map_err(|error| fail(error.into(), None))?;
            store.lock().map_err(|error| fail(error.into(), None))?;
        }
    } else if !store.is_locked() || !peer.is_locked() {
        return Err(fail(
            PipelineError::Store(StoreError::Conflict(
                "all cross-store participants must be prelocked".into(),
            )),
            None,
        ));
    }
    store
        .require_write_authority()
        .map_err(|error| fail(error.into(), None))?;
    store
        .check_expected(expected)
        .map_err(|error| fail(error.into(), None))?;
    crate::records::cross::validate_peer_locked(store, peer, peer_store_id, peer_expected)
        .map_err(|error| fail(error.into(), None))?;
    if peer_store_id != peer.state().store_id {
        return Err(fail(
            PipelineError::Commit(format!(
                "selected peer identity differs from endpoint: {peer_store_id}"
            )),
            None,
        ));
    }
    if !crate::relations::node::is_range_key(local_endpoint) {
        return Err(fail(
            PipelineError::Commit("external link local endpoint must be a range".into()),
            None,
        ));
    }
    if !store.state().peers.contains_key(peer_store_id) {
        return Err(fail(
            PipelineError::Commit(format!("peer store not registered: {peer_store_id}")),
            None,
        ));
    }
    store.state().tips.get(local_endpoint).ok_or_else(|| {
        fail(
            PipelineError::Commit(format!("link local range does not exist: {local_endpoint}")),
            None,
        )
    })?;
    crate::relations::identity::resolve_version_on_chain(store, local_endpoint, local_version)
        .map_err(|error| {
            fail(
                PipelineError::Commit(format!("link local version: {error}")),
                None,
            )
        })?;
    if !crate::relations::node::is_range_key(peer_endpoint)
        || !peer.state().tips.contains_key(peer_endpoint)
    {
        return Err(fail(
            PipelineError::Commit(format!(
                "peer range does not exist: {peer_endpoint} @ {peer_store_id}"
            )),
            None,
        ));
    }
    crate::relations::identity::resolve_version_on_chain(peer, peer_endpoint, peer_version)
        .map_err(|error| {
            fail(
                PipelineError::Commit(format!("peer endpoint version: {error}")),
                None,
            )
        })?;
    if link_id.len() < 8 {
        return Err(fail(
            PipelineError::Commit("cross-store link id is invalid".into()),
            None,
        ));
    }
    let planned = plan_xlink(
        store,
        rng,
        clock,
        node_key,
        local_endpoint,
        local_version,
        peer_store_id,
        peer_endpoint,
        peer_version,
        link_id,
        reason,
        direction,
    )
    .map_err(|error| fail(error, None))?;
    let consumer_registration_id = peer
        .state()
        .peers
        .get(&store.state().store_id)
        .cloned()
        .ok_or_else(|| {
            fail(
                PipelineError::Store(StoreError::Conflict(format!(
                    "peer {peer_store_id} has no registration for consumer {}",
                    store.state().store_id
                ))),
                None,
            )
        })?;
    let credential_id = crate::records::cross::persist_inbound(
        peer,
        &store.identity().project_id,
        &store.state().store_id,
        &consumer_registration_id,
        &planned.record_id,
        &planned.link_id,
        peer_endpoint,
        peer_version,
    )
    .map_err(|error| fail(error.into(), None))?;
    let link_id = publish_planned_xlink(store, probe, planned)
        .map_err(|error| fail(error, Some(credential_id.clone())))?;
    Ok(ProtectedXlink {
        link_id,
        credential_id,
    })
}

/// Form the exact immutable business record before peer protection. The
/// returned record ID is persisted in the peer receipt, then this same commit
/// and state are published without regenerating salt, time, or identity.
#[allow(clippy::too_many_arguments)]
fn plan_xlink(
    store: &Store,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    local_endpoint: &str,
    local_version: &str,
    peer_store_id: &str,
    peer_endpoint: &str,
    peer_version: &str,
    link_id: &str,
    reason: &str,
    direction: ExternalLinkDirection,
) -> Result<PlannedXlink, PipelineError> {
    let link_id = link_id.to_string();
    let peer_key = crate::relations::node::peer_key(peer_store_id, peer_endpoint);
    let (source, source_version, target, target_version) = match direction {
        ExternalLinkDirection::PeerToLocal => (
            peer_key,
            peer_version.to_string(),
            local_endpoint.to_string(),
            local_version.to_string(),
        ),
        ExternalLinkDirection::LocalToPeer => (
            local_endpoint.to_string(),
            local_version.to_string(),
            peer_key,
            peer_version.to_string(),
        ),
    };
    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation {
        bytes: Vec::new(),
        text: false,
        encoding: None,
    };
    let version = make_version(
        rng,
        &empty_obs,
        Acquisition::File {
            project: "root".into(),
            path: "".into(),
        },
    );
    let prev = store
        .state()
        .tips
        .get(node_key)
        .cloned()
        .unwrap_or_default();
    let mut payload = serde_json::Map::new();
    payload.insert("link_id".into(), link_id.clone().into());
    payload.insert("source".into(), source.clone().into());
    payload.insert("source_version".into(), source_version.clone().into());
    payload.insert("target".into(), target.clone().into());
    payload.insert("target_version".into(), target_version.clone().into());
    payload.insert("peer_store_id".into(), peer_store_id.into());
    payload.insert("reason".into(), reason.into());
    let mut commit = make_commit(rng, clock, CommitKind::Link, &prev, &version, payload);
    commit.content_ref = "empty".into();
    commit
        .validate(is_first)
        .map_err(|error| PipelineError::Commit(error.to_string()))?;
    let record_id = commit
        .derive_id(b"")
        .map_err(|error| PipelineError::Commit(error.to_string()))?
        .to_hex();

    let mut state = store.state().clone();
    state.publication += 1;
    state.tips.insert(node_key.to_string(), record_id.clone());
    state.retained.push(record_id.clone());
    state.links.insert(
        link_id.clone(),
        crate::records::store::Link {
            link_id: link_id.clone(),
            source,
            target,
            source_version,
            target_version,
            created_by: record_id.clone(),
        },
    );
    state.link_pending.entry(link_id.clone()).or_default();
    Ok(PlannedXlink {
        link_id,
        record_id,
        commit,
        state,
    })
}

fn publish_planned_xlink(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    planned: PlannedXlink,
) -> Result<String, PipelineError> {
    store.publish(
        probe,
        &planned.commit,
        &planned.record_id,
        None,
        None,
        planned.state,
    )?;
    Ok(planned.link_id)
}

/// Adapt: handle selected changes on a link with an explicit reason.
/// Requires link_id + changes + reason — all three, never guessed.
#[allow(clippy::too_many_arguments)]
pub fn commit_adapt(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    link_id: &str,
    changes: &[String],
    reason: &str,
    stop: bool,
    no_reason: bool,
    expected: &Expected,
) -> Result<String, PipelineError> {
    if link_id.is_empty() {
        return Err(PipelineError::Commit(
            "adapt requires an explicit link_id".into(),
        ));
    }
    if reason.is_empty() && !(stop && no_reason) {
        return Err(PipelineError::Commit("adapt requires a reason".into()));
    }
    if changes.is_empty() {
        return Err(PipelineError::Commit(
            "adapt/stop requires explicitly selected changes".into(),
        ));
    }
    store.lock()?;
    store.require_write_authority()?;
    store.check_expected(expected)?;
    if !store.state().links.contains_key(link_id) {
        return Err(PipelineError::Commit(format!("unknown link_id: {link_id}")));
    }

    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation {
        bytes: Vec::new(),
        text: false,
        encoding: None,
    };
    let version = make_version(
        rng,
        &empty_obs,
        Acquisition::File {
            project: "root".into(),
            path: "".into(),
        },
    );
    let prev = store
        .state()
        .tips
        .get(node_key)
        .cloned()
        .unwrap_or_default();
    let mut payload = serde_json::Map::new();
    payload.insert("link_id".into(), link_id.into());
    payload.insert(
        "changes".into(),
        changes
            .iter()
            .cloned()
            .map(serde_json::Value::from)
            .collect(),
    );
    if no_reason {
        payload.insert("no_reason".into(), true.into());
    } else {
        payload.insert("reason".into(), reason.into());
    }
    payload.insert("stop".into(), stop.into());
    let commit = make_commit(rng, clock, CommitKind::Adapt, &prev, &version, payload);
    // Structural link commits consume no source bytes — content_ref is
    // "empty"; a phantom version id would point at a record never written.
    let commit = {
        let mut c = commit;
        c.content_ref = "empty".into();
        c
    };
    commit
        .validate(is_first)
        .map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit
        .derive_id(b"")
        .map_err(|e| PipelineError::Commit(e.to_string()))?
        .to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    // Adapt/stop acknowledges exactly selected pending obligations.
    if let Some(pend) = new_state.link_pending.get_mut(link_id) {
        for change in changes {
            pend.remove(change);
        }
    }
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(cid)
}

/// Immutable fields needed to decide whether a reset target is eligible.
#[derive(Debug, Clone)]
pub struct ResetTargetRecord {
    pub kind: CommitKind,
    pub previous_id: String,
    pub parent_block: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResetBlockMembership {
    pub scope: &'static str,
    pub block: String,
}

/// Shared reset eligibility: marker targets are legal boundaries; ordinary
/// targets must be outside both their own chain's block and any parent-file
/// block recorded on the immutable range commit.
pub fn reset_block_membership<F>(
    target: &str,
    lookup: &mut F,
) -> Result<Option<ResetBlockMembership>, ResetError>
where
    F: FnMut(&str) -> Option<ResetTargetRecord>,
{
    let target_record = lookup(target).ok_or_else(|| ResetError::UnknownTarget(target.into()))?;
    if matches!(
        target_record.kind,
        CommitKind::AtomicBegin | CommitKind::AtomicEnd
    ) {
        return Ok(None);
    }
    if let Some(block) = target_record.parent_block {
        return Ok(Some(ResetBlockMembership {
            scope: "parent-file",
            block,
        }));
    }
    let mut depth = 0i64;
    let mut cur = target.to_string();
    let mut guard = 0usize;
    while let Some(record) = lookup(&cur) {
        match record.kind {
            CommitKind::AtomicEnd => depth += 1,
            CommitKind::AtomicBegin => {
                if depth == 0 {
                    return Ok(Some(ResetBlockMembership {
                        scope: "same-chain",
                        block: cur,
                    }));
                }
                depth -= 1;
            }
            _ => {}
        }
        if record.previous_id.is_empty() || guard > 100_000 {
            break;
        }
        guard += 1;
        cur = record.previous_id;
    }
    Ok(None)
}

pub fn file_reset_children_checked<F>(
    range_tips: &std::collections::BTreeMap<String, String>,
    mut child_membership: F,
) -> Result<Vec<(String, String)>, ResetError>
where
    F: FnMut(&str) -> Result<Option<ResetBlockMembership>, ResetError>,
{
    let mut restored = Vec::new();
    for (range_id, tip) in range_tips {
        if let Some(membership) = child_membership(tip)? {
            return Err(ResetError::ChildInterior {
                child: range_id.clone(),
                target: tip.clone(),
                scope: membership.scope,
                block: membership.block,
            });
        }
        restored.push((range_id.clone(), tip.clone()));
    }
    Ok(restored)
}

/// Compatibility wrapper for callers that already determined only a boolean
/// same-chain membership. New reset paths use `file_reset_children_checked`.
pub fn file_reset_children<F>(
    range_tips: &std::collections::BTreeMap<String, String>,
    mut child_is_interior: F,
) -> Result<Vec<(String, String)>, ResetError>
where
    F: FnMut(&str) -> bool,
{
    file_reset_children_checked(range_tips, |tip| {
        Ok(child_is_interior(tip).then(|| ResetBlockMembership {
            scope: "same-chain",
            block: "unknown".into(),
        }))
    })
}

/// Outcome of a `reset <id>`: where it landed and the machine-readable
/// warning when the requested boundary differs from the actual landing.
#[derive(Debug, serde::Serialize)]
pub struct ResetOutcome {
    pub requested: String,
    /// "" = withdrew to nothing (empty chain / unmounted).
    pub actual: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub warning: String,
}

/// Execute `reset <id>` on `node_key`. Reads each commit's kind + previous_id
/// from disk; marker targets land on their direct predecessor (one step only),
/// interior ordinary members are refused, and the reset itself is recorded as
/// a commit (tip moves; history is never deleted). `lookup` resolves a commit
/// id to (kind, previous_id).
pub fn reset<F>(_node_key: &str, target: &str, mut lookup: F) -> Result<ResetOutcome, ResetError>
where
    F: FnMut(&str) -> Option<ResetTargetRecord>,
{
    let record = lookup(target).ok_or_else(|| ResetError::UnknownTarget(target.into()))?;
    match record.kind {
        CommitKind::AtomicBegin | CommitKind::AtomicEnd => {
            // Marker: withdraw it and its successors, land on direct
            // predecessor — exactly one step, never skipped recursively.
            Ok(ResetOutcome {
                requested: target.to_string(),
                actual: record.previous_id.clone(),
                warning: format!(
                    "requested boundary {target} lands on predecessor {}",
                    if record.previous_id.is_empty() {
                        "<empty>".into()
                    } else {
                        record.previous_id.clone()
                    }
                ),
            })
        }
        _ => {
            if let Some(membership) = reset_block_membership(target, &mut lookup)? {
                return Err(ResetError::Interior {
                    target: target.into(),
                    scope: membership.scope,
                    block: membership.block,
                });
            }
            // Ordinary target outside a block: kept; successors dangle.
            Ok(ResetOutcome {
                requested: target.to_string(),
                actual: target.to_string(),
                warning: String::new(),
            })
        }
    }
}

/// Walk ancestors of `commit` to detect whether it sits inside an ATOMIC
/// block: an unmatched BEGIN before it (with no matching END before it too)
/// makes it a block member.
pub fn commit_is_block_member<F>(commit: &str, lookup: &mut F) -> bool
where
    F: FnMut(&str) -> Option<(CommitKind, String)>,
{
    let mut depth = 0i64;
    let mut cur = commit.to_string();
    let mut guard = 0usize;
    while let Some((k, prev)) = lookup(&cur) {
        match k {
            CommitKind::AtomicEnd => depth += 1,
            CommitKind::AtomicBegin => {
                if depth == 0 {
                    return true; // an unmatched BEGIN before commit
                }
                depth -= 1;
            }
            _ => {}
        }
        if prev.is_empty() || guard > 100_000 {
            break;
        }
        guard += 1;
        cur = prev;
    }
    false
}

#[derive(Debug, thiserror::Error)]
pub enum ResetError {
    #[error("unknown reset target: {0}")]
    UnknownTarget(String),
    #[error("reset target is an ordinary block member: {target} ({scope} block {block})")]
    Interior {
        target: String,
        scope: &'static str,
        block: String,
    },
    #[error(
        "child {child} restore target {target} is an ordinary block member ({scope} block {block})"
    )]
    ChildInterior {
        child: String,
        target: String,
        scope: &'static str,
        block: String,
    },
}

/// Apply a resolved reset to the node's state: move the tip to the actual
/// landing point, mark every commit removed by the reset as dangling, and
/// withdraw link/adapt records that were *created* inside the removed
/// segment (they stop counting as current; the records stay readable).
///
/// `lookup` resolves a commit id → (kind, previous_id). `state` is mutated
/// in place by the caller after `publish`.
pub fn apply_reset_to_state<F>(
    state: &mut crate::records::store::State,
    node_key: &str,
    requested: &str,
    actual: &str,
    mut lookup: F,
) where
    F: FnMut(&str) -> Option<(CommitKind, String)>,
{
    // The removed segment = commits between the OLD tip and the landing
    // point — i.e. the OLD tip's descendants down to `actual`. Capture the
    // old tip BEFORE moving it, then walk old-tip → actual.
    let old_tip = state.tips.get(node_key).cloned().unwrap_or_default();
    // Move the node tip to the landing point (empty = withdraw chain).
    if actual.is_empty() {
        state.tips.remove(node_key);
    } else {
        state.tips.insert(node_key.to_string(), actual.to_string());
    }
    // Commits removed by the reset = old-tip → actual (successor direction).
    let mut removed: Vec<String> = Vec::new();
    let mut cur = old_tip;
    let mut guard = 0usize;
    while !cur.is_empty() && cur != actual && guard < 100_000 {
        removed.push(cur.clone());
        cur = lookup(&cur).map(|(_, p)| p).unwrap_or_default();
        guard += 1;
    }
    let _ = requested; // landing-point arg kept for the report
    // Withdraw link/adapt records whose *creating* commit is in `removed`.
    // A link's creation commit id is its value's `created_by` field.
    let removed_set: std::collections::BTreeSet<&String> = removed.iter().collect();
    state
        .links
        .retain(|_id, link| !removed_set.contains(&link.created_by));
    state
        .link_pending
        .retain(|id, _| state.links.contains_key(id));
    // Adapt records are commits too — withdrawn commits no longer count as
    // current processing evidence. (Pending re-derives from live links.)
    for pend in state.link_pending.values_mut() {
        pend.retain(|c| !removed_set.contains(c));
    }
}

#[derive(Debug)]
enum RangeAssessment {
    Clean,
    Dirty(String),
    Locate(String),
    Unverified(String),
    NoBody,
}

/// Compare one range's effective body with its current authoritative file
/// location. Both verify and FileVerify use this exact fold/compare path.
fn assess_range(store: &Store, range_key: &str, current_bytes: &[u8]) -> RangeAssessment {
    let state = match crate::relations::identity::effective_range_state(store, range_key) {
        Ok(state) => state,
        Err(error) => return RangeAssessment::Unverified(error.to_string()),
    };
    let (version_id, range) = match (state.source_version_id, state.range) {
        (Some(version), Some(range)) => (version, range),
        _ => return RangeAssessment::NoBody,
    };
    let version = match store.read_version(&version_id) {
        Ok(version) => version,
        Err(_) => {
            return RangeAssessment::Unverified(format!("version record missing ({version_id})"));
        }
    };
    let old_bytes = match store.recover_version_bytes(&version_id) {
        Ok(bytes) => bytes,
        Err(error) => return RangeAssessment::Unverified(error.to_string()),
    };

    let (candidates, hunks) = match range.mode {
        crate::relations::range::Mode::Byte => {
            if range.end > old_bytes.len() as u64 {
                return RangeAssessment::Unverified(format!(
                    "range out of bounds for recorded source version {version_id}"
                ));
            }
            let fragment = &old_bytes[range.start as usize..range.end as usize];
            (
                crate::relations::diff::locate_byte_candidates(fragment, current_bytes),
                crate::relations::diff::diff_bytes(&old_bytes, current_bytes),
            )
        }
        crate::relations::range::Mode::Text => {
            let encoding = version.encoding.as_deref().unwrap_or("utf-8");
            let old = match crate::sources::decode(&old_bytes, encoding) {
                Ok(text) => text,
                Err(_) => {
                    return RangeAssessment::Unverified(format!(
                        "recorded content undecodable as {encoding}"
                    ));
                }
            };
            let current = match crate::sources::decode(current_bytes, encoding) {
                Ok(text) => text,
                Err(_) => {
                    return RangeAssessment::Unverified(format!(
                        "current source cannot be decoded as {encoding}"
                    ));
                }
            };
            if range.end > crate::relations::range::text_len(&old) {
                return RangeAssessment::Unverified(format!(
                    "range out of bounds for recorded source version {version_id}"
                ));
            }
            let fragment = match crate::relations::range::text_slice(&old, &range) {
                Ok(fragment) => fragment,
                Err(_) => {
                    return RangeAssessment::Unverified(format!(
                        "range out of bounds for recorded source version {version_id}"
                    ));
                }
            };
            (
                crate::relations::diff::locate_candidates(fragment, &current),
                crate::relations::diff::diff_text(&old, &current),
            )
        }
    };

    if candidates.len() > 1 {
        return RangeAssessment::Locate(format!(
            "ambiguous: {} candidates for effective range",
            candidates.len()
        ));
    }
    if candidates.len() == 1 && candidates[0] != range.start as usize {
        return RangeAssessment::Locate(format!(
            "moved: fragment now at {} (was {}), needs review",
            candidates[0], range.start
        ));
    }
    if crate::relations::diff::dirtied_by(&hunks, &[range])[0] {
        return RangeAssessment::Dirty(format!("in-range edit at {}", state.tip_id));
    }
    RangeAssessment::Clean
}

#[derive(Debug, Clone)]
pub struct SuccessfulObservation {
    pub bytes: Vec<u8>,
    pub acquisition: Acquisition,
    pub encoding: Option<String>,
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
    /// Locate problems: a range whose recorded fragment is ambiguous in the
    /// current source — reports old coords + candidates, never auto-picks.
    pub locate: std::collections::BTreeMap<String, Vec<String>>,
    /// Tracked file paths that vanished without a tombstone — `missing`,
    /// never auto-deleted nor silently OK.
    pub missing: Vec<String>,
    /// Command-sourced versions verify could NOT check because running the
    /// command wasn't permitted (`may_run` false). `unverified` is honest
    /// incomplete state — never counted as pass, never a silent failure.
    pub unverified: Vec<String>,
    /// Selected project identity failures. Non-empty blocks all source
    /// acquisition and business writes until explicit registration repair.
    pub identity: Vec<String>,
    /// Successful command acquisitions available for exact reuse by a later
    /// write. Skipped from report serialization; CLI persists versions and
    /// emits their IDs in `expected`.
    #[serde(skip)]
    pub successful_observations: std::collections::BTreeMap<String, SuccessfulObservation>,
}

/// Run verify against the persisted state — reads what's on disk, never
/// fabricates a passing result from a moving target. `run_cmd` is the
/// invocation's `--run-command` decision (from `may_run`); command-sourced
/// versions report `unverified` when running isn't permitted.
pub fn verify(
    store: &Store,
    project_root: &std::path::Path,
    run_cmd: bool,
    encoding_override: Option<&str>,
) -> VerifyReport {
    let st = store.state();
    let open_blocks: Vec<String> = st
        .open_blocks
        .iter()
        .filter(|(_, values)| !values.is_empty())
        .map(|(node, _)| node.clone())
        .collect();
    let mut obligations = Vec::new();
    let mut dirty = std::collections::BTreeMap::new();
    for (node, state) in &st.dirty {
        if !state.obligations.is_empty() {
            obligations.push(node.clone());
        }
        let ids: Vec<String> = state.dirty.keys().cloned().collect();
        if !ids.is_empty() {
            dirty.insert(node.clone(), ids);
        }
    }
    let identity = store.identity_diagnostic().into_iter().collect::<Vec<_>>();
    if !identity.is_empty() {
        return VerifyReport {
            ok: false,
            open_blocks,
            obligations,
            dirty,
            locate: Default::default(),
            missing: Vec::new(),
            unverified: identity.clone(),
            identity,
            successful_observations: Default::default(),
        };
    }

    let config_cwd = store.config_cwd().unwrap_or(project_root);
    let mut cache = std::collections::BTreeMap::<Acquisition, Result<Vec<u8>, String>>::new();
    let mut successful_observations = std::collections::BTreeMap::new();
    let mut current = std::collections::BTreeMap::<String, Vec<u8>>::new();
    let mut unverified = Vec::new();
    let mut missing = Vec::new();
    let mut missing_seen = std::collections::BTreeSet::new();

    for node in st.tips.keys() {
        let version_id = match store.source_version_id(node) {
            Ok(Some(version_id)) => version_id,
            Ok(None) => continue,
            Err(error) => {
                unverified.push(format!("{node}: {error}"));
                continue;
            }
        };
        let version = match store.read_version(&version_id) {
            Ok(version) => version,
            Err(error) => {
                unverified.push(format!(
                    "{node}: version record missing ({version_id}): {error}"
                ));
                continue;
            }
        };
        let Some(descriptor) = store.current_acquisition(node, &version) else {
            unverified.push(format!("{node}: current source location is unavailable"));
            continue;
        };
        let logical_path = crate::relations::node::path_of(st, node)
            .or_else(|| {
                crate::relations::node::parent_of(st, node)
                    .and_then(|parent| crate::relations::node::path_of(st, parent))
            })
            .unwrap_or("");
        let encoding = if version.encoding.is_some() {
            match crate::sources::encoding::resolve_runtime(
                encoding_override,
                version.encoding.as_deref(),
                config_cwd,
                store.root(),
                logical_path,
            ) {
                Ok(encoding) => Some(encoding),
                Err(error) => {
                    unverified.push(format!("{node}: {error}"));
                    continue;
                }
            }
        } else {
            None
        };
        if matches!(descriptor, Acquisition::Command { .. }) && !run_cmd {
            unverified.push(format!("{node} (command source, not run)"));
            continue;
        }
        let collected = cache.entry(descriptor.clone()).or_insert_with(|| {
            crate::sources::collect(&descriptor, config_cwd, project_root, false, None)
                .map(|observation| observation.bytes)
                .map_err(|error| error.to_string())
        });
        let bytes = match collected {
            Ok(bytes) => bytes.clone(),
            Err(error) => {
                if let Acquisition::File { path, .. } = &descriptor
                    && crate::relations::node::is_file_key(node)
                    && missing_seen.insert(path.clone())
                {
                    missing.push(format!("{path} (no tombstone)"));
                }
                unverified.push(format!("{node}: {error}"));
                continue;
            }
        };
        match store.recover_version_bytes(&version.id.to_hex()) {
            Ok(recorded) => {
                if matches!(descriptor, Acquisition::Command { .. }) && recorded != bytes {
                    dirty
                        .entry(node.clone())
                        .or_insert_with(Vec::new)
                        .push(format!("command output changed ({})", st.tips[node]));
                }
            }
            Err(error) => unverified.push(format!("{node}: {error}")),
        }
        current.insert(node.clone(), bytes.clone());
        successful_observations.insert(
            node.clone(),
            SuccessfulObservation {
                bytes,
                acquisition: descriptor,
                encoding,
            },
        );
    }

    let mut locate = std::collections::BTreeMap::new();
    for range_key in st
        .tips
        .keys()
        .filter(|key| crate::relations::node::is_range_key(key))
    {
        let Some(bytes) = current.get(range_key) else {
            continue;
        };
        match assess_range(store, range_key, bytes) {
            RangeAssessment::Clean | RangeAssessment::NoBody => {}
            RangeAssessment::Dirty(message) => {
                dirty.entry(range_key.clone()).or_default().push(message);
            }
            RangeAssessment::Locate(message) => {
                locate
                    .entry(range_key.clone())
                    .or_insert_with(Vec::new)
                    .push(message);
            }
            RangeAssessment::Unverified(message) => {
                unverified.push(format!("{range_key}: {message}"));
            }
        }
    }

    let ok = open_blocks.is_empty()
        && obligations.is_empty()
        && dirty.is_empty()
        && locate.is_empty()
        && missing.is_empty()
        && unverified.is_empty();
    VerifyReport {
        ok,
        open_blocks,
        obligations,
        dirty,
        locate,
        missing,
        unverified,
        identity,
        successful_observations,
    }
}

/// Recompute whether a range node's effective body still matches the current
/// authoritative file location. Missing/corrupt evidence blocks verification;
/// a first-BEGIN-only range has no body and is handled by open-block state.
pub fn range_needs_review(store: &Store, project_root: &std::path::Path, range_key: &str) -> bool {
    let Some(version) = node_source_version(store, range_key) else {
        return true;
    };
    let Some(descriptor) = store.current_acquisition(range_key, &version) else {
        return true;
    };
    if !matches!(descriptor, Acquisition::File { .. }) {
        return true;
    }
    let config_cwd = store.config_cwd().unwrap_or(project_root);
    let Ok(observation) =
        crate::sources::collect(&descriptor, config_cwd, project_root, false, None)
    else {
        return true;
    };
    !matches!(
        assess_range(store, range_key, &observation.bytes),
        RangeAssessment::Clean | RangeAssessment::NoBody
    )
}
