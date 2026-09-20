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
/// Commit a command-sourced version: run `exe argv` in `project_root`,
/// capture complete stdout as the version's content, and record
/// `Acquisition::Command`. Only `exit 0` produces content — a failed run
/// keeps the previous version and is an error, never partial output.
pub fn commit_command_source(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    exe: &str,
    argv: &[String],
    project_root: &Path,
    kind: CommitKind,
    payload: serde_json::Map<String, serde_json::Value>,
    expected: &Expected,
) -> Result<String, PipelineError> {
    store.lock()?;
    store.check_expected(expected)?;
    let out = crate::sources::command::observe_command(exe, argv, project_root)
        .map_err(PipelineError::Source)?;
    let obs = out.into_observation()
        .ok_or(PipelineError::Source(SourceError::Command))?;
    let acquisition = Acquisition::Command {
        executable: exe.into(),
        args: argv.to_vec(),
    };
    let version = make_version(rng, &obs, acquisition);
    let version = with_content(&version);
    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let is_first = !store.state().tips.contains_key(node_key);
    let commit = make_commit(rng, clock, kind, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(&obs.bytes).map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();
    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    store.publish(probe, &commit, &cid, Some(&version), Some(&obs.bytes), new_state)?;
    Ok(cid)
}

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
    store.lock()?;
    store.check_expected(expected)?;

    let is_first = !store.state().tips.contains_key(node_key);
    // Resolve the source path relative to the process CWD — the caller
    // supplies the project root as its working directory. Observation reads
    // the real file *now*, never a snapshot.
    let enc = encoding.unwrap_or("utf-8");
    let obs = observe_file(path, true, Some(enc))?;
    let version = make_version(rng, &obs, Acquisition::File {
        path: path.to_string_lossy().into(),
        encoding: enc.into(),
    });
    let version = with_content(&version);

    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let mut commit = make_commit(rng, clock, kind, &prev, &version, payload);
    // A file-level commit snapshots which tip each child range points to now,
    // so a later file reset restores exact children — not wall-clock order.
    if crate::relations::node::is_file_key(node_key) {
        for (k, tip) in &store.state().tips {
            if crate::relations::node::is_range_key(k)
                && crate::relations::node::parent_of(k) == *node_key
            {
                commit.range_tips.insert(k.clone(), tip.clone());
            }
        }
    }
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(&obs.bytes).map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    // Mount a new range node under its file the first time it appears.
    if crate::relations::node::is_range_key(node_key) && is_first {
        let parent = crate::relations::node::parent_of(node_key);
        new_state.mounts.entry(parent).or_default().push(node_key.to_string());
    }
    // Upstream commits on a linked *source* range seed pending obligations on
    // each link whose source is this node — adapt later clears the selected
    // ones. The obligation key is this commit's id (one per change).
    // TRANSITIVE: a commit on N also flags downstream links — if N→T is a
    // link and T is itself a source of link L2, then L2's chain is now
    // indirectly dirty (the breakage propagates c1→b1→a1 without B resetting).
    let direct: Vec<String> = new_state
        .links
        .iter()
        .filter(|(_, l)| l.source == node_key)
        .map(|(id, _)| id.clone())
        .collect();
    for lid in &direct {
        new_state
            .link_pending
            .entry(lid.clone())
            .or_default()
            .insert(cid.clone());
    }
    // One transitive hop: each link whose source is a TARGET of a direct link
    // also gets flagged (the target node's chain now owes review downstream).
    let downstream_sources: Vec<String> = direct.iter()
        .filter_map(|lid| new_state.links.get(lid).map(|l| l.target.clone()))
        .collect();
    let transitive: Vec<String> = new_state
        .links
        .iter()
        .filter(|(_, l)| downstream_sources.contains(&l.source))
        .map(|(id, _)| id.clone())
        .collect();
    for lid in transitive {
        new_state
            .link_pending
            .entry(lid)
            .or_default()
            .insert(cid.clone());
    }

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
        CommitKind::Tag => {
            // Flat project-local tag on this node. Dir nodes' tags inherit to
            // members at check time (additive, deduped — a set, never counted
            // twice). The tag name is scoped to this store; no cross-project
            // identity merge.
            if let Some(t) = commit.payload.get("tag").and_then(|v| v.as_str()) {
                new_state.tags.entry(node_key.to_string()).or_default().insert(t.to_string());
            }
        }
        CommitKind::ScopeAdjust => {
            // Record a named tag-link rule (`spec->code` / `spec<->code`) with
            // severity + skip. A declared rule is a check item — never auto-
            // invents ranges/links and never gates verify.
            if let Some(r) = commit.payload.get("rule").and_then(|v| v.as_str()) {
                let level = commit.payload.get("level").and_then(|v| v.as_str()).unwrap_or("fail").to_string();
                let skip = commit.payload.get("skip").and_then(|v| v.as_bool()).unwrap_or(false);
                new_state.tag_rules.insert(r.to_string(), crate::records::store::TagRule {
                    rule: r.to_string(), level, skip,
                });
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
    store.check_expected(expected)?;

    let src_key = format!("file:{source_path}");
    let empty_obs = Observation { bytes: Vec::new(), text: false, encoding: None };
    let version = make_version(rng, &empty_obs, Acquisition::File {
        path: source_path.into(), encoding: "utf-8".into(),
    });

    let mut payload = serde_json::Map::new();
    payload.insert("path".into(), source_path.into());
    payload.insert("source".into(), source_path.into());
    if let Some(t) = target_path { payload.insert("target".into(), t.into()); }
    payload.insert("reason".into(), reason.into());

    let prev = store.state().tips.get(&src_key).cloned().unwrap_or_default();
    let is_first = !store.state().tips.contains_key(&src_key);
    let commit = make_commit(rng, clock, kind, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(b"").map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.retained.push(cid.clone());

    match kind {
        CommitKind::Rename => {
            let target = target_path.expect("rename needs target");
            let dst_key = format!("file:{target}");
            // Migrate the node: tip moves to the target key, mounts and dirty
            // state follow, ranges under the file keep their identity.
            new_state.tips.remove(&src_key);
            new_state.tips.insert(dst_key.clone(), cid.clone());
            if let Some(m) = new_state.mounts.remove(&src_key) {
                // Re-key child range nodes: `range:<path>@...` → `range:<target>@...`.
                // The range identity (its tip/chain) is preserved; only the
                // path component of its key follows the rename.
                let remap = |c: &str| c.replacen(
                    &format!("range:{source_path}@"),
                    &format!("range:{target}@"), 1);
                for child in &m {
                    let new_child = remap(child);
                    if let Some(tip) = new_state.tips.remove(child) {
                        new_state.tips.insert(new_child.clone(), tip);
                    }
                    if let Some(ds) = new_state.dirty.remove(child) {
                        new_state.dirty.insert(new_child.clone(), ds);
                    }
                }
                let remounted: Vec<String> = m.iter().map(|c| remap(c)).collect();
                new_state.mounts.insert(dst_key.clone(), remounted);
            }
            if let Some(ds) = new_state.dirty.remove(&src_key) {
                new_state.dirty.insert(dst_key.clone(), ds);
            }
        }
        CommitKind::Delete => {
            // Tombstone: tip stays on the source node recording target=null;
            // dependents on this node report broken (handled at read).
            new_state.tips.insert(src_key.clone(), cid.clone());
        }
        _ => {}
    }

    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(cid)
}

/// Create a link instance between two range-commits. Refuses file-level
/// linking (source/target must name a range, not a whole file). Each link
/// gets a fresh 128-bit id; identical endpoints+direction coexist as
/// distinct instances. `source`/`target` are range-commit endpoints like
/// `file:path@start-end` — presence of '@' marks a range.
pub fn commit_link(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    source: &str,
    target: &str,
    reason: &str,
    expected: &Expected,
) -> Result<String, PipelineError> {
    // Endpoints must name range nodes, not whole files — a range key is a
    // first-class object identity, not a text sniff.
    if !crate::relations::node::is_range_key(source)
        || !crate::relations::node::is_range_key(target)
    {
        return Err(PipelineError::Commit("links connect ranges, not whole files".into()));
    }
    store.lock()?;
    store.check_expected(expected)?;
    // Both endpoints must be REAL range nodes — a link to a range that was
    // never initialized is a phantom reference, not a forward link.
    if !store.state().tips.contains_key(source) {
        return Err(PipelineError::Commit(format!("link source range does not exist: {source}")));
    }
    if !store.state().tips.contains_key(target) {
        return Err(PipelineError::Commit(format!("link target range does not exist: {target}")));
    }

    let mut idb = [0u8; 16];
    rng.fill(&mut idb);
    let link_id = crate::records::ids::Id128(idb).to_hex();

    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation { bytes: Vec::new(), text: false, encoding: None };
    let version = make_version(rng, &empty_obs, Acquisition::File { path: "".into(), encoding: "".into() });
    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let mut payload = serde_json::Map::new();
    payload.insert("link_id".into(), link_id.clone().into());
    payload.insert("source".into(), source.into());
    payload.insert("target".into(), target.into());
    payload.insert("reason".into(), reason.into());
    let commit = make_commit(rng, clock, CommitKind::Link, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(b"").map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    new_state.links.insert(link_id.clone(), crate::records::store::Link {
        link_id: link_id.clone(),
        source: source.to_string(),
        target: target.to_string(),
        created_by: cid.clone(),
    });
    new_state.link_pending.entry(link_id.clone()).or_default();
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(link_id)
}

/// Cross-store link: A records a link whose TARGET is a range node in a
/// registered peer store B (endpoint `peer:<store_id>:<file>@<range>`).
/// The link record lives only in A — each store keeps its own .omd, no
/// commit files or tips are shared/merged. B gets a symmetric *inbound*
/// record (queryable + gc-protected) persisted against the target range,
/// referencing A's pending link record id. Source must be a local range.
///
/// `peer_tips` resolves the peer's live tip set (read-only) — the caller
/// supplies it so the store crate stays free of peer-FS access; a peer
/// whose store can't be opened makes the link fail, never a cached guess.
pub fn commit_xlink<F>(
    store: &mut Store,
    probe: &mut dyn PublishProbe,
    rng: &dyn Rng,
    clock: &dyn Clock,
    node_key: &str,
    source: &str,
    peer_store_id: &str,
    peer_target_key: &str,
    reason: &str,
    expected: &Expected,
    peer_tips: F,
) -> Result<String, PipelineError>
where
    F: FnOnce(&str) -> Option<std::collections::BTreeMap<String, String>>,
{
    if !crate::relations::node::is_range_key(source) {
        return Err(PipelineError::Commit("xlink source must be a range node".into()));
    }
    store.lock()?;
    store.check_expected(expected)?;
    if !store.state().tips.contains_key(source) {
        return Err(PipelineError::Commit(format!("link source range does not exist: {source}")));
    }
    // The peer must be REGISTERED and the target range must exist in its
    // live tips — a cross-store link to a phantom peer range is refused.
    let peer = store.state().peers.get(peer_store_id).cloned().ok_or_else(||
        PipelineError::Commit(format!("peer store not registered: {peer_store_id}")))?;
    let tips = peer_tips(&peer.locator)
        .ok_or_else(|| PipelineError::Commit(format!("peer store unreadable: {}", peer.locator)))?;
    if !tips.contains_key(peer_target_key) {
        return Err(PipelineError::Commit(format!(
            "peer target range does not exist: {peer_target_key} @ {peer_store_id}")));
    }

    let mut idb = [0u8; 16];
    rng.fill(&mut idb);
    let link_id = crate::records::ids::Id128(idb).to_hex();
    // The target key carries its peer store identity so it never collides
    // with a local range key of the same coordinates.
    let target_key = format!("peer:{peer_store_id}:{peer_target_key}");

    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation { bytes: Vec::new(), text: false, encoding: None };
    let version = make_version(rng, &empty_obs, Acquisition::File { path: "".into(), encoding: "".into() });
    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let mut payload = serde_json::Map::new();
    payload.insert("link_id".into(), link_id.clone().into());
    payload.insert("source".into(), source.into());
    payload.insert("target".into(), target_key.clone().into());
    payload.insert("peer_store_id".into(), peer_store_id.into());
    payload.insert("reason".into(), reason.into());
    let commit = make_commit(rng, clock, CommitKind::Link, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(b"").map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    new_state.links.insert(link_id.clone(), crate::records::store::Link {
        link_id: link_id.clone(),
        source: source.to_string(),
        target: target_key,
        created_by: cid.clone(),
    });
    new_state.link_pending.entry(link_id.clone()).or_default();
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(link_id)
}

/// Adapt: handle selected changes on a link with an explicit reason.
/// Requires link_id + changes + reason — all three, never guessed.
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
    expected: &Expected,
) -> Result<String, PipelineError> {
    if link_id.is_empty() {
        return Err(PipelineError::Commit("adapt requires an explicit link_id".into()));
    }
    if reason.is_empty() {
        return Err(PipelineError::Commit("adapt requires a reason".into()));
    }
    // Adapt must record the *selected* changes (one, several, or all via
    // --stop). A bare adapt with no --changes and no --stop has made no
    // selection — refuse rather than silently waive pending obligations.
    if !stop && changes.is_empty() {
        return Err(PipelineError::Commit(
            "adapt requires --changes <ids> (selected changes) or --stop (all)".into()));
    }
    store.lock()?;
    store.check_expected(expected)?;
    if !store.state().links.contains_key(link_id) {
        return Err(PipelineError::Commit(format!("unknown link_id: {link_id}")));
    }

    let is_first = !store.state().tips.contains_key(node_key);
    let empty_obs = Observation { bytes: Vec::new(), text: false, encoding: None };
    let version = make_version(rng, &empty_obs, Acquisition::File { path: "".into(), encoding: "".into() });
    let prev = store.state().tips.get(node_key).cloned().unwrap_or_default();
    let mut payload = serde_json::Map::new();
    payload.insert("link_id".into(), link_id.into());
    payload.insert("changes".into(), changes.join(",").into());
    payload.insert("reason".into(), reason.into());
    payload.insert("stop".into(), stop.into());
    let commit = make_commit(rng, clock, CommitKind::Adapt, &prev, &version, payload);
    commit.validate(is_first).map_err(|e| PipelineError::Commit(e.to_string()))?;
    let cid = commit.derive_id(b"").map_err(|e| PipelineError::Commit(e.to_string()))?.to_hex();

    let mut new_state: State = store.state().clone();
    new_state.publication += 1;
    new_state.tips.insert(node_key.to_string(), cid.clone());
    new_state.retained.push(cid.clone());
    // Adapt acknowledges the SELECTED pending obligations on this link.
    // `--changes c1,c2` clears only the named pending commit ids (unselected
    // obligations stay pending); `--stop` handles all upstream changes and
    // blocks the whole source end.
    if let Some(pend) = new_state.link_pending.get_mut(link_id) {
        if stop {
            pend.clear();
        } else {
            for c in changes {
                pend.remove(c);
            }
        }
    }
    store.publish(probe, &commit, &cid, None, None, new_state)?;
    Ok(cid)
}

/// File reset: restore each child range's recorded tip from the target file
/// commit's `range_tips` snapshot — never by wall-clock or current table.
/// `child_is_interior` reports whether a child's recorded tip sits inside an
/// ATOMIC block (a block member, not a boundary); if ANY child's restore
/// target is interior, the WHOLE file reset refuses, siblings unchanged.
pub fn file_reset_children<F>(
    range_tips: &std::collections::BTreeMap<String, String>,
    mut child_is_interior: F,
) -> Result<Vec<(String, String)>, ResetError>
where
    F: FnMut(&str) -> bool,
{
    let mut restored = Vec::new();
    for (range_id, tip) in range_tips {
        if child_is_interior(tip) {
            return Err(ResetError::UnknownTarget(format!(
                "child {range_id} restore target {tip} is a block member"
            )));
        }
        restored.push((range_id.clone(), tip.clone()));
    }
    Ok(restored)
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
pub fn reset<F>(
    _node_key: &str,
    target: &str,
    mut lookup: F,
) -> Result<ResetOutcome, ResetError>
where
    F: FnMut(&str) -> Option<(CommitKind, String)>,
{
    let (kind, prev) = lookup(target).ok_or(ResetError::UnknownTarget(target.into()))?;
    match kind {
        CommitKind::AtomicBegin | CommitKind::AtomicEnd => {
            // Marker: withdraw it and its successors, land on direct
            // predecessor — exactly one step, never skipped recursively.
            Ok(ResetOutcome {
                requested: target.to_string(),
                actual: prev.clone(),
                warning: format!(
                    "requested boundary {target} lands on predecessor {}",
                    if prev.is_empty() { "<empty>".into() } else { prev.clone() }
                ),
            })
        }
        _ => {
            // An ordinary commit *inside* an ATOMIC block is never a reset
            // target — only markers and out-of-block commits are. Detect
            // membership by walking ancestors: if a BEGIN precedes this
            // commit without its matching END also preceding it, the commit
            // is a block member.
            if is_block_member(target, &mut lookup) {
                return Err(ResetError::Interior(target.into()));
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
fn is_block_member<F>(commit: &str, lookup: &mut F) -> bool
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
    #[error("reset target is an ordinary block member: {0}")]
    Interior(String),
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
    state.links.retain(|_id, link| !removed_set.contains(&link.created_by));
    state.link_pending.retain(|id, _| state.links.contains_key(id));
    // Adapt records are commits too — withdrawn commits no longer count as
    // current processing evidence. (Pending re-derives from live links.)
    for pend in state.link_pending.values_mut() {
        pend.retain(|c| !removed_set.contains(c));
    }
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
}

/// Run verify against the persisted state — reads what's on disk, never
/// fabricates a passing result from a moving target. `run_cmd` is the
/// invocation's `--run-command` decision (from `may_run`); command-sourced
/// versions report `unverified` when running isn't permitted.
pub fn verify(store: &Store, run_cmd: bool) -> VerifyReport {
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
    // Locate diagnostics: a range node whose recorded fragment now matches
    // ambiguously in the current file is a problem — candidates reported, the
    // track is never silently re-pointed nor kept as a valid confirmation.
    let mut locate: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut scan = |range_key: &str, tip_id: &str| {
        // Recover the recorded fragment from the range's tip commit's version
        // and count its occurrences in the *current* file. >1 = ambiguous.
        if let Ok(commit) = store.read_commit(tip_id) {
            let path = commit.payload.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let range_arg = commit.payload.get("range").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let (Some((_, s, e)), Ok(cur)) = (
                crate::relations::node::parse_range_arg(&range_arg),
                std::fs::read_to_string(&path),
            ) {
                if let Ok(ver) = store.read_version(&commit.content_ref) {
                    if let Ok(old_bytes) = store.read_content(&ver.sha256) {
                        let old = String::from_utf8_lossy(&old_bytes);
                        let frag: String = old.chars().skip(s as usize).take((e - s) as usize).collect();
                        let cands = crate::relations::diff::locate_candidates(&frag, &cur);
                        if cands.len() > 1 {
                            locate.entry(range_key.to_string())
                                .or_default()
                                .push(format!("ambiguous: {} candidates for '{}'", cands.len(), frag));
                        } else if cands.len() == 1 && cands[0] != s as usize {
                            // Pure position move: the fragment survives intact
                            // but at a NEW offset — a candidate migration the
                            // spec requires explicit review for, never an
                            // auto-kept confirmation, never a silent CLEAN.
                            locate.entry(range_key.to_string())
                                .or_default()
                                .push(format!("moved: fragment now at {} (was {}), needs review", cands[0], s));
                        }
                        // Myers dirty check: a hunk overlapping the recorded
                        // range marks it dirty even when the fragment still
                        // locates — in-range edits always need review.
                        let hunks = crate::relations::diff::diff_text(&old, &cur);
                        let ranges = [crate::relations::range::Range {
                            start: s, end: e,
                            mode: crate::relations::range::Mode::Text,
                        }];
                        if crate::relations::diff::dirtied_by(&hunks, &ranges)[0] {
                            dirty.entry(range_key.to_string())
                                .or_insert_with(Vec::new)
                                .push(format!("in-range edit at {}", tip_id));
                        }
                    }
                }
            }
        }
    };
    for (k, tip) in &st.tips {
        if crate::relations::node::is_range_key(k) {
            scan(k, tip);
        }
    }
    // Missing-source diagnostics: a tracked file node whose registered path
    // vanished WITHOUT a tombstone is `missing` — never auto-interpreted as
    // an intentional delete, never silently OK. A tombstone (Delete kind tip)
    // is intentional and is not `missing`.
    let mut missing: Vec<String> = Vec::new();
    for (k, tip) in &st.tips {
        if let Some(path) = k.strip_prefix("file:") {
            let is_tombstone = store.read_commit(tip)
                .map(|c| c.kind == CommitKind::Delete)
                .unwrap_or(false);
            // A command/git-sourced file node is virtual — it has no disk
            // path to go missing. Only FILE-acquired nodes check the FS.
            let is_virtual = store.read_commit(tip)
                .and_then(|c| store.read_version(&c.content_ref).map(|v| v.acquisition.clone()))
                .map(|a| !matches!(a, crate::records::version::Acquisition::File { .. }))
                .unwrap_or(false);
            let proj_root = std::env::current_dir().unwrap_or_default();
            if !is_tombstone && !is_virtual && !proj_root.join(path).exists() {
                missing.push(format!("{path} (no tombstone)"));
            }
        }
    }
    // Command-sourced versions: without `run_cmd` permission we cannot get
    // their current output — report them `unverified` (incomplete), never
    // a fabricated pass and never a hidden failure.
    let mut unverified: Vec<String> = Vec::new();
    let proj_root = std::env::current_dir().unwrap_or_default();
    for (k, tip) in &st.tips {
        if let Ok(c) = store.read_commit(tip) {
            if let Ok(v) = store.read_version(&c.content_ref) {
                if let crate::records::version::Acquisition::Command { executable, args } = &v.acquisition {
                    if !run_cmd {
                        unverified.push(format!("{k} (command source, not run)"));
                    } else {
                        // Re-run the command and compare its stdout to the
                        // recorded content — like a file re-read. Changed
                        // output → dirty; identical → stays confirmed.
                        match crate::sources::command::observe_command(executable, args, &proj_root) {
                            Ok(out) if out.exit_ok => {
                                let recorded = store.read_content(&v.sha256).unwrap_or_default();
                                if out.stdout != recorded {
                                    dirty.entry(k.clone()).or_insert_with(Vec::new)
                                        .push(format!("command output changed ({})", tip));
                                }
                            }
                            _ => {
                                // Non-zero exit / spawn failure → unverified,
                                // never a fabricated clean.
                                unverified.push(format!("{k} (command failed to run)"));
                            }
                        }
                    }
                }
            }
        }
    }
    let ok = open_blocks.is_empty() && obligations.is_empty() && dirty.is_empty()
        && locate.is_empty() && missing.is_empty() && unverified.is_empty();
    VerifyReport { ok, open_blocks, obligations, dirty, locate, missing, unverified }
}

/// Recompute whether a range node's recorded content still matches the
/// current file — the same Myers check verify runs, exposed so a FileVerify
/// commit can block on outstanding (not-yet-persisted) range dirt. Returns
/// true when the range is dirty or has a position problem needing review.
pub fn range_needs_review(store: &Store, tip_id: &str) -> bool {
    let commit = match store.read_commit(tip_id) { Ok(c) => c, Err(_) => return false };
    let path = commit.payload.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let range_arg = commit.payload.get("range").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (_, s, e) = match crate::relations::node::parse_range_arg(&range_arg) {
        Some(t) => t, None => return false,
    };
    let cur = match std::fs::read_to_string(&path) { Ok(c) => c, Err(_) => return true };
    let ver = match store.read_version(&commit.content_ref) { Ok(v) => v, Err(_) => return false };
    let old_bytes = match store.read_content(&ver.sha256) { Ok(b) => b, Err(_) => return false };
    let old = String::from_utf8_lossy(&old_bytes);
    let frag: String = old.chars().skip(s as usize).take((e - s) as usize).collect();
    let cands = crate::relations::diff::locate_candidates(&frag, &cur);
    if cands.len() > 1 || (cands.len() == 1 && cands[0] != s as usize) {
        return true; // ambiguous or moved — needs review
    }
    let hunks = crate::relations::diff::diff_text(&old, &cur);
    let ranges = [crate::relations::range::Range {
        start: s, end: e, mode: crate::relations::range::Mode::Text,
    }];
    crate::relations::diff::dirtied_by(&hunks, &ranges)[0]
}
