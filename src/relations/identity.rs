//! Structured object identity — spec `structured-tracking-references`.
//!
//! A *node* is one of three object kinds: a file/command/git tracked
//! object (`Kind::File`), a tracked range object (`Kind::Range`), or a
//! peer-store endpoint reference (`Kind::Peer`). A node's identity is its
//! **chain root** — the id of its first commit (the `Init`/`AtomicBegin`
//! marker). That id survives coordinate moves, renames, and content edits:
//! the chain is the object.
//!
//! A *reference* is a user/CLI selection that resolves to a node and a
//! chain position. `resolve_ref` turns a `RefSpec` into a `Resolved`
//! — the object id, current tip, selected commit, and latest same-chain
//! range-body commit (distinct from structural BEGIN/END/link tips).
//!
//! Positions carry *separate literal fields* — never a `proj:`/`command::`/
//! `git::` string. `Position` is a tagged enum over source kinds; each
//! variant holds only the fields that kind needs.

use serde::{Deserialize, Serialize};

use crate::records::commit::CommitKind;
use crate::records::store::{Store, StoreError};
use crate::relations::range::Mode;

/// A tracked object's kind. File covers command/git sources too — the
/// *source kind* lives in `Position`, not the object kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// File/command/git object (whole-source tracking unit).
    File,
    /// Range object — a tracked span inside a file/command/git source.
    Range,
    /// Peer-store endpoint reference (cross-store link target).
    Peer,
    /// Audit journal — append-only linear chain, own root.
    Audit,
    /// Note thread — append-only linear chain, own root.
    Note,
}

/// A reference to a *node* (an object, not a commit on its chain).
/// `kind` + `id` are the persisted form; `id` is the chain-root commit id
/// of the node's first commit. The state key is `<kind>:<id>` —
/// `Display`/`FromStr` provide that canonical wire form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Ref {
    pub kind: Kind,
    /// Chain-root commit id — the node's first commit's id. For `Peer`,
    /// the peer endpoint key (`<store>:<remote-root-id>`).
    pub id: String,
}

impl Ref {
    pub fn file(id: impl Into<String>) -> Self {
        Self {
            kind: Kind::File,
            id: id.into(),
        }
    }
    pub fn range(id: impl Into<String>) -> Self {
        Self {
            kind: Kind::Range,
            id: id.into(),
        }
    }
    /// A peer endpoint: `id` is the peer's chain-root id of the target
    /// range node; `peer` is the peer store_id.
    pub fn peer(peer_store_id: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: Kind::Peer,
            id: format!("{}:{}", peer_store_id.into(), id.into()),
        }
    }
    /// The state key form: `<kind>:<id>`.
    pub fn key(&self) -> String {
        self.to_string()
    }
}

impl std::fmt::Display for Ref {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", kind_name(self.kind), self.id)
    }
}

impl std::str::FromStr for Ref {
    type Err = RefError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, id) = s.split_once(':').ok_or(RefError::BadForm)?;
        let kind = parse_kind(kind).ok_or(RefError::BadForm)?;
        if id.is_empty() {
            return Err(RefError::BadForm);
        }
        Ok(Self {
            kind,
            id: id.to_string(),
        })
    }
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::File => "file",
        Kind::Range => "range",
        Kind::Peer => "peer",
        Kind::Audit => "audit",
        Kind::Note => "note",
    }
}

pub fn parse_kind(s: &str) -> Option<Kind> {
    match s {
        "file" => Some(Kind::File),
        "range" => Some(Kind::Range),
        "peer" => Some(Kind::Peer),
        "audit" => Some(Kind::Audit),
        "note" => Some(Kind::Note),
        _ => None,
    }
}

/// A range's half-open coordinate span, in its own unit.
/// Serialized as the two-element array `[start, end]` so TOML and JSON
/// agree on the same wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Span(pub u64, pub u64);

/// A position — the source coordinates a node observes or a range covers.
/// Separate literal fields, never a compound URI. `deny_unknown_fields`
/// rejects keys outside the active variant so a malformed record fails
/// fast instead of silently dropping a coordinate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Position {
    /// A file selected through a project alias.
    /// `path` is project-relative; `encoding` defaults to utf-8.
    File {
        project: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encoding: Option<String>,
    },
    /// A virtual file produced by running a fixed executable.
    /// `args` are literal strings — never a shell string, never
    /// substituted.
    Command {
        executable: String,
        args: Vec<String>,
    },
    /// An exact-commit blob selected through a project alias. Remote
    /// constraints belong to project registration, never source coordinates.
    Git {
        project: String,
        commit: String,
        path: String,
    },
}

/// A range selection: coordinate span + unit.
/// `span`/`unit` are the two required halves; `unit` defaults to text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeSel {
    pub span: Span,
    #[serde(default = "default_mode")]
    pub unit: Mode,
}

fn default_mode() -> Mode {
    Mode::Text
}

/// A *structured reference* — the fields a CLI flag or payload carries to
/// name a node and select a position/version.
///
/// For `kind = range`, `position` is the parent file's source coordinates
/// (the object the range lives inside) and `range` is the span it covers.
/// `version`/`expect_version`/`link_id` carry commit-level selection:
/// `version` pins a historical commit; `expect_version` asserts the chain's
/// current effective tip; `link_id` names the link instance an adapt
/// operates on.
///
/// For `kind = peer`, `store` is the peer store_id and `id` is the remote
/// node's chain-root id; `position`/`range` are unused (a peer endpoint is
/// a remote node identity, not local coordinates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefSpec {
    pub kind: Kind,
    /// Object id. For `range`/`file`: the chain-root commit id, or "" when
    /// the caller hasn't resolved it yet (the store fills it from the
    /// node table). For `peer`: the peer endpoint key.
    pub id: String,
    /// Source coordinates of the *object* — the file/command/git it reads.
    /// Absent for peer endpoints and for ranges that inherit the parent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Range span + unit; only for `kind = range`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<RangeSel>,
    /// A peer's store_id (for `kind = peer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    /// Historical commit to select (full or unique-prefix commit id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Assert the chain's current effective tip equals this commit id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_version: Option<String>,
    /// Link instance id (adapt targets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_id: Option<String>,
}

/// Parse a `<kind>:<id>` node-key string into a `Ref`. This is the *key*
/// form persisted in state — not a user-facing URI.
pub fn parse_ref_arg(s: &str) -> Option<Ref> {
    s.parse().ok()
}

/// The result of resolving a `RefSpec` against a store's live chain.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The node's canonical reference.
    pub node: Ref,
    /// The node's current tip commit id ("" when the node has no commits).
    pub tip: String,
    /// The selected commit: `version` when given, else `tip`.
    pub selected: String,
    /// For a range node: the latest same-chain commit that supplies its
    /// effective position and source version. Empty when the chain contains
    /// only structural markers (for example, its first BEGIN).
    pub effective_range_commit_id: String,
}

/// Effective persisted state of one range object. Identity, structural tip,
/// body commit/source version, position, and current parent location remain
/// separate facts.
#[derive(Debug, Clone)]
pub struct EffectiveRangeState {
    pub node_key: String,
    pub root_commit_id: String,
    pub tip_id: String,
    pub effective_range_commit_id: Option<String>,
    pub source_version_id: Option<String>,
    pub range: Option<crate::relations::range::Range>,
    pub current_parent_key: String,
    pub current_path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EffectiveRangeError {
    #[error("unknown range node: {0}")]
    UnknownNode(String),
    #[error("range has no mounted file: {0}")]
    MissingParent(String),
    #[error("invalid range position on commit {0}")]
    InvalidPosition(String),
    #[error(transparent)]
    Ref(#[from] RefError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Debug, thiserror::Error)]
pub enum RefError {
    #[error("bad ref form (expected <kind>:<id>)")]
    BadForm,
    #[error("unknown node: {0}")]
    UnknownNode(String),
    #[error("unknown version: {0}")]
    UnknownVersion(String),
    #[error("ambiguous commit prefix: {0}")]
    AmbiguousVersion(String),
    #[error("missing predecessor {missing} while resolving {commit}")]
    MissingPredecessor { commit: String, missing: String },
    #[error("cycle in commit ancestry at {0}")]
    AncestryCycle(String),
    #[error("commit {0} is not a retained published record")]
    NotRetained(String),
    #[error("expected version mismatch: want {0}, current {1}")]
    VersionMismatch(String, String),
    #[error("not a range node: {0}")]
    NotRange(String),
    #[error("kind mismatch: expected {expected}, got {got}")]
    KindMismatch { expected: String, got: String },
}

/// Resolve a `RefSpec` against the store's live node table.
///
/// - `spec.id` is the node's chain-root id (first commit's id). For a
///   `file`/`range` node the id is the chain root; for `peer`, the peer
///   endpoint key. A prefix is accepted only when it uniquely identifies
///   one live node — an ambiguous prefix is an error, never a fuzzy match.
/// - `spec.version` selects a historical commit on that node's chain
///   (full id or unique prefix among the chain's commit ids).
/// - `spec.expect_version` asserts the chain's *effective* tip — for a
///   range, the tip recorded in the parent's `range_tips` snapshot.
/// - Returns the current tip, selected commit, and effective same-chain
///   range-body commit (if any).
pub fn resolve_ref(store: &Store, spec: &RefSpec) -> Result<Resolved, RefError> {
    let (node, node_key) = resolve_node_ref(store, spec)?;
    let tip = store
        .state()
        .tips
        .get(&node_key)
        .cloned()
        .ok_or_else(|| RefError::UnknownNode(node_key.clone()))?;
    let selected = match &spec.version {
        Some(version) => resolve_version_on_chain(store, &node_key, version)?,
        None => tip.clone(),
    };
    let effective_range_commit_id = if node.kind == Kind::Range {
        effective_range_state(store, &node_key)
            .map_err(|error| RefError::UnknownVersion(error.to_string()))?
            .effective_range_commit_id
            .unwrap_or_default()
    } else {
        tip.clone()
    };
    if let Some(expected) = &spec.expect_version
        && expected != &tip
    {
        return Err(RefError::VersionMismatch(expected.clone(), tip));
    }
    Ok(Resolved {
        node,
        tip,
        selected,
        effective_range_commit_id,
    })
}

fn resolve_node_ref(store: &Store, spec: &RefSpec) -> Result<(Ref, String), RefError> {
    let kind = kind_name(spec.kind);
    let exact = format!("{kind}:{}", spec.id);
    if store.state().tips.contains_key(&exact) {
        return Ok((
            Ref {
                kind: spec.kind,
                id: spec.id.clone(),
            },
            exact,
        ));
    }
    let prefix = format!("{kind}:{}", spec.id);
    let mut roots: Vec<String> = store
        .state()
        .tips
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .cloned()
        .collect();
    roots.sort();
    roots.dedup();
    match roots.len() {
        1 => {
            let key = roots.pop().unwrap();
            let id = key.split_once(':').unwrap().1.to_string();
            return Ok((
                Ref {
                    kind: spec.kind,
                    id,
                },
                key,
            ));
        }
        n if n > 1 => return Err(RefError::AmbiguousVersion(spec.id.clone())),
        _ => {}
    }
    let (key, root) = commit_to_node(store, &spec.id)?;
    let got = key.split_once(':').map(|(kind, _)| kind).unwrap_or("");
    if got != kind {
        return Err(RefError::KindMismatch {
            expected: kind.into(),
            got: got.into(),
        });
    }
    Ok((
        Ref {
            kind: spec.kind,
            id: root,
        },
        key,
    ))
}

/// Resolve a full retained commit id or store-wide unique prefix.
pub fn resolve_commit_id(store: &Store, value: &str) -> Result<String, RefError> {
    if store.state().retained.iter().any(|id| id == value) {
        return Ok(value.to_string());
    }
    let mut matches: Vec<String> = store
        .state()
        .retained
        .iter()
        .filter(|id| id.starts_with(value))
        .cloned()
        .collect();
    matches.sort();
    matches.dedup();
    match matches.len() {
        1 => Ok(matches.pop().unwrap()),
        0 => Err(RefError::UnknownVersion(value.into())),
        _ => Err(RefError::AmbiguousVersion(value.into())),
    }
}

/// Immutable facts derived from one object's selected ancestry. Mutable state
/// may project these facts, but cannot redefine them.
#[derive(Debug, Clone)]
pub struct AuthoritativeObject {
    pub kind: Kind,
    pub root_commit_id: String,
    pub location: Option<String>,
    pub parent: Option<String>,
}

/// Walk `tip` to its root and derive object kind, active file location, and
/// range mount solely from immutable commits.
pub fn authoritative_object(store: &Store, tip: &str) -> Result<AuthoritativeObject, RefError> {
    let mut chain = Vec::new();
    let mut current = tip.to_string();
    let mut visited = std::collections::BTreeSet::new();
    while !current.is_empty() {
        if !visited.insert(current.clone()) {
            return Err(RefError::AncestryCycle(current));
        }
        let commit = store
            .read_commit(&current)
            .map_err(|_| RefError::MissingPredecessor {
                commit: tip.into(),
                missing: current.clone(),
            })?;
        chain.push((current.clone(), commit.clone()));
        current = commit.previous_id;
    }
    let (root_commit_id, root) = chain
        .last()
        .ok_or_else(|| RefError::UnknownVersion(tip.into()))?;
    let parent = root
        .payload
        .get("mount")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let kind = if parent.is_some() {
        Kind::Range
    } else if matches!(root.kind, CommitKind::Init | CommitKind::Import) {
        Kind::File
    } else if root.kind == CommitKind::AuditInit {
        Kind::Audit
    } else if root.kind == CommitKind::NoteInit {
        Kind::Note
    } else {
        return Err(RefError::UnknownNode(root_commit_id.clone()));
    };
    let location = if kind == Kind::File {
        let mut location = None;
        for (_, commit) in chain.iter().rev() {
            match commit.kind {
                CommitKind::Delete => location = None,
                CommitKind::Rename => {
                    location = commit
                        .payload
                        .get("target")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                }
                _ if location.is_none() => {
                    location = commit
                        .payload
                        .get("path")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                }
                _ => {}
            }
        }
        location
    } else {
        None
    };
    Ok(AuthoritativeObject {
        kind,
        root_commit_id: root_commit_id.clone(),
        location,
        parent,
    })
}

/// Strictly walk one immutable chain and return its root id. Missing records
/// and cycles are errors; neither an index nor a guard limit may manufacture a
/// plausible root.
pub fn chain_root(store: &Store, commit_id: &str) -> Result<String, RefError> {
    let mut current = commit_id.to_string();
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return Err(RefError::AncestryCycle(current));
        }
        let commit = store
            .read_commit(&current)
            .map_err(|_| RefError::MissingPredecessor {
                commit: commit_id.into(),
                missing: current.clone(),
            })?;
        if commit.previous_id.is_empty() {
            return Ok(current);
        }
        current = commit.previous_id;
    }
}

pub(crate) fn kind_for_root(store: &Store, root: &str) -> Result<Kind, RefError> {
    Ok(authoritative_object(store, root)?.kind)
}

/// Resolve any retained commit (including dangling history) to object kind,
/// chain root, and canonical object key. Resolution alone does not make that
/// commit writable; callers separately compare it with the live tip.
pub fn commit_to_node(store: &Store, commit_id: &str) -> Result<(String, String), RefError> {
    let selected = resolve_commit_id(store, commit_id)?;
    let root = chain_root(store, &selected)?;
    let kind = kind_for_root(store, &root)?;
    let key = match kind {
        Kind::File => crate::relations::node::file_key(&root),
        Kind::Range => crate::relations::node::range_key(&root),
        Kind::Audit => crate::relations::node::audit_key(&root),
        Kind::Note => crate::relations::node::note_key(&root),
        Kind::Peer => unreachable!(),
    };
    Ok((key, root))
}

/// Resolve a retained full/unique-prefix commit and require that immutable
/// ancestry identifies the requested object. The selected commit may be
/// dangling; write callers must still compare it to the current tip.
pub fn resolve_version_on_chain(
    store: &Store,
    node_key: &str,
    value: &str,
) -> Result<String, RefError> {
    let selected = resolve_commit_id(store, value)?;
    let selected_root = chain_root(store, &selected)?;
    let expected_root = node_key
        .split_once(':')
        .map(|(_, id)| id)
        .ok_or(RefError::BadForm)?;
    if selected_root != expected_root {
        return Err(RefError::UnknownVersion(value.into()));
    }
    Ok(selected)
}

/// Fold one range chain into the facts every consumer needs. The live tip may
/// be a structural marker or link; the effective body is the newest same-chain
/// ancestor carrying both a source version and a range position.
pub fn effective_range_state(
    store: &Store,
    node_key: &str,
) -> Result<EffectiveRangeState, EffectiveRangeError> {
    if !crate::relations::node::is_range_key(node_key) {
        return Err(EffectiveRangeError::UnknownNode(node_key.into()));
    }
    let tip_id = store
        .state()
        .tips
        .get(node_key)
        .cloned()
        .ok_or_else(|| EffectiveRangeError::UnknownNode(node_key.into()))?;
    let root_commit_id = chain_root(store, &tip_id)?;
    if crate::relations::node::range_key(&root_commit_id) != node_key {
        return Err(EffectiveRangeError::UnknownNode(node_key.into()));
    }
    let current_parent_key = crate::relations::node::parent_of(store.state(), node_key)
        .ok_or_else(|| EffectiveRangeError::MissingParent(node_key.into()))?
        .to_string();
    let current_path = crate::relations::node::path_of(store.state(), &current_parent_key)
        .ok_or_else(|| EffectiveRangeError::MissingParent(node_key.into()))?
        .to_string();

    let mut current = tip_id.clone();
    let mut visited = std::collections::BTreeSet::new();
    let mut effective = None;
    while !current.is_empty() {
        if !visited.insert(current.clone()) {
            return Err(EffectiveRangeError::Ref(RefError::AncestryCycle(current)));
        }
        let commit = store.read_commit(&current)?;
        if commit.content_ref != "empty"
            && let Some(range) = commit
                .payload
                .get("position")
                .and_then(crate::relations::node::position_from_value)
        {
            effective = Some((current.clone(), commit.content_ref.clone(), range));
            break;
        }
        current = commit.previous_id;
    }
    let (effective_range_commit_id, source_version_id, range) = match effective {
        Some((commit, version, range)) => (Some(commit), Some(version), Some(range)),
        None => (None, None, None),
    };
    Ok(EffectiveRangeState {
        node_key: node_key.into(),
        root_commit_id,
        tip_id,
        effective_range_commit_id,
        source_version_id,
        range,
        current_parent_key,
        current_path,
    })
}

pub fn is_marker_commit(store: &Store, node_key: &str, commit_id: &str) -> bool {
    resolve_version_on_chain(store, node_key, commit_id)
        .and_then(|id| {
            store
                .read_commit(&id)
                .map_err(|_| RefError::UnknownVersion(id))
        })
        .map(|commit| matches!(commit.kind, CommitKind::AtomicBegin | CommitKind::AtomicEnd))
        .unwrap_or(false)
}
