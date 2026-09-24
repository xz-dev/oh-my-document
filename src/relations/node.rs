//! Object keys and derived current locations.
//!
//! Persistent file/range identity is always `<kind>:<chain-root-commit-id>`.
//! File paths live in `State::locations`; ranges mount under file object keys.
//! Nothing recovers identity by parsing a path or coordinate from a key.

use crate::records::store::State;
use crate::relations::range::{Mode, Range};

/// Persisted range position. Numeric fields stay decimal strings so canonical
/// JSON never loses precision; consumers convert directly to the shared Range.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub mode: Mode,
    pub start: String,
    pub end: String,
}

impl From<Range> for Position {
    fn from(range: Range) -> Self {
        Self {
            mode: range.mode,
            start: range.start.to_string(),
            end: range.end.to_string(),
        }
    }
}

impl TryFrom<Position> for Range {
    type Error = &'static str;

    fn try_from(position: Position) -> Result<Self, Self::Error> {
        let start = position
            .start
            .parse()
            .map_err(|_| "invalid position start")?;
        let end = position.end.parse().map_err(|_| "invalid position end")?;
        if end < start {
            return Err("position end precedes start");
        }
        Ok(Self {
            start,
            end,
            mode: position.mode,
        })
    }
}

pub fn position_value(range: Range) -> serde_json::Value {
    serde_json::to_value(Position::from(range)).expect("position is serializable")
}

pub fn position_from_value(value: &serde_json::Value) -> Option<Range> {
    serde_json::from_value::<Position>(value.clone())
        .ok()
        .and_then(|position| position.try_into().ok())
}

/// Key for a file object rooted at `root_commit_id`.
pub fn file_key(root_commit_id: &str) -> String {
    format!("file:{root_commit_id}")
}

/// Key for a range object rooted at `root_commit_id`.
pub fn range_key(root_commit_id: &str) -> String {
    format!("range:{root_commit_id}")
}

/// State-only lookup cannot distinguish content from import roots. Return
/// only a unique match; business callers use Store's typed path lookup.
pub fn file_at_path<'a>(state: &'a State, path: &str) -> Option<&'a str> {
    let mut matches = state.locations.iter().filter_map(|(node, current)| {
        (current == path && is_file_key(node)).then_some(node.as_str())
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Current project-relative path for a file object.
pub fn path_of<'a>(state: &'a State, node: &str) -> Option<&'a str> {
    state.locations.get(node).map(String::as_str)
}

/// Key for a peer endpoint: `peer:<store-id>:<remote node key>`.
pub fn peer_key(store_id: &str, remote_node_key: &str) -> String {
    format!("peer:{store_id}:{remote_node_key}")
}

/// Does `key` name a range object?
pub fn is_range_key(key: &str) -> bool {
    key.starts_with("range:")
}

/// Does `key` name a file object?
pub fn is_file_key(key: &str) -> bool {
    key.starts_with("file:")
}

/// Is `key` a peer endpoint reference?
pub fn is_peer_key(key: &str) -> bool {
    key.starts_with("peer:")
}

/// Key for an audit object rooted at `root_commit_id`.
pub fn audit_key(root_commit_id: &str) -> String {
    format!("audit:{root_commit_id}")
}

/// Key for a note object rooted at `root_commit_id`.
pub fn note_key(root_commit_id: &str) -> String {
    format!("note:{root_commit_id}")
}

/// Does `key` name an audit object?
pub fn is_audit_key(key: &str) -> bool {
    key.starts_with("audit:")
}

/// Does `key` name a note object?
pub fn is_note_key(key: &str) -> bool {
    key.starts_with("note:")
}

/// Does `key` name an append-only journal object (audit or note)?
/// These chains share the range/file commit machinery but are linear
/// roots — never mounted under a file.
pub fn is_journal_key(key: &str) -> bool {
    is_audit_key(key) || is_note_key(key)
}

/// The node a range mounts under — looked up in the mount tree, never
/// parsed out of the key (the key carries no location).
pub fn parent_of<'a>(state: &'a State, key: &str) -> Option<&'a str> {
    for (parent, children) in &state.mounts {
        if children.iter().any(|c| c == key) {
            return Some(parent.as_str());
        }
    }
    None
}
