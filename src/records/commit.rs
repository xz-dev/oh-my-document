//! Immutable commit records (design E-2).
//!
//! A commit is the unit of business history. Its ID covers salt, previous_id,
//! timestamp, full source content, and the canonical operation payload — in
//! that exact order and field framing. The file on disk additionally carries
//! top-level `schema`/`kind`/`content_ref` projections for indexing; those
//! projections MUST match the values injected into the canonical payload, or
//! the record is rejected (no side-channel fields can change meaning without
//! changing the ID).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::records::id::{self, CommitId, ContentRef, OperationPayload};
use crate::records::ids::Id128;

/// Commit operation kinds. Closed set; unknown kinds are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitKind {
    /// First record of a file/command/git object (path.target only).
    Init,
    /// Ordinary content/state commit (includes `clean`/`unclean`).
    Commit,
    /// Explicit source-side stop (`commit clean`).
    Clean,
    /// Explicit dirty obligation (`commit unclean`).
    Unclean,
    /// ATOMIC begin marker (placeholder).
    AtomicBegin,
    /// ATOMIC end marker (placeholder).
    AtomicEnd,
    /// Link creation (with link_id in payload).
    Link,
    /// Adaptation handling a link's selected changes.
    Adapt,
    /// Path rename (source→target).
    Rename,
    /// Tombstone (target null).
    Delete,
    /// Import statistics inclusion.
    Import,
    /// Import statistics removal.
    Remove,
    /// Exclude/include scope adjustment.
    ScopeAdjust,
    /// Tag assignment.
    Tag,
    /// File-hash verification commit (`commit verify <path>`).
    FileVerify,
    /// Reset marker (records requested/actual landing).
    Reset,
}

/// On-disk record. `payload_fields` is the closed per-kind operation map;
/// `schema`, `kind`, and `content_ref` are injected into the canonical
/// payload — the top-level fields are projections that MUST agree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    /// Derived ID (not stored; recomputed and checked on read).
    #[serde(skip)]
    pub id: Option<CommitId>,

    /// 16-char `[A-Za-z0-9]` salt, first hash input.
    pub salt: String,

    /// Previous commit on this node's chain; `""` for the first.
    #[serde(default)]
    pub previous_id: String,

    /// Canonical UTC RFC3339 nanosecond timestamp.
    pub timestamp: String,

    /// Top-level projections — must equal the values injected into payload.
    pub schema: String,
    pub kind: CommitKind,
    pub content_ref: String,

    /// Closed operation fields (paths, coordinates, reasons, endpoints,
    /// link ids, expected versions, source hashes). Unknown keys for the
    /// kind are rejected by `validate` in later tasks; the map is closed
    /// per-kind.
    #[serde(default)]
    pub payload: serde_json::Map<String, serde_json::Value>,

    /// File-commit snapshot: which tip each child range pointed to when this
    /// file commit was recorded (`range_id -> tip`). Required on file-level
    /// commits (init/file_verify) so a file reset restores exact children
    /// without guessing from wall-clock order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub range_tips: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    #[error("salt must be 16 [A-Za-z0-9] chars")]
    BadSalt,
    #[error("timestamp must be UTC RFC3339 nanoseconds")]
    BadTimestamp,
    #[error("payload projection mismatch: {0}")]
    Projection(&'static str),
    #[error("first commit of a node must have empty previous_id")]
    BadFirstPrevious,
    #[error("unknown payload field for this kind: {0}")]
    UnknownField(String),
    #[error("missing required payload field: {0}")]
    MissingField(&'static str),
}

impl Commit {
    /// Canonical timestamp format used everywhere: UTC, nanosecond precision.
    pub fn format_timestamp(nanos: u64) -> String {
        let secs = (nanos / 1_000_000_000) as i64;
        let ns = (nanos % 1_000_000_000) as u32;
        let dt = time::OffsetDateTime::from_unix_timestamp(secs)
            .unwrap()
            .replace_nanosecond(ns)
            .unwrap();
        dt.format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    }

    /// The canonical operation payload with projections injected.
    pub fn canonical_payload(&self) -> OperationPayload {
        let mut fields = self.payload.clone();
        if !self.range_tips.is_empty() {
            let tips: serde_json::Map<String, serde_json::Value> = self
                .range_tips
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            fields.insert("range_tips".into(), serde_json::Value::Object(tips));
        }
        OperationPayload {
            kind: kind_name(self.kind).to_string(),
            schema: self.schema.clone(),
            content_ref: parse_content_ref(&self.content_ref),
            fields,
        }
    }

    /// Derive this record's commit ID from its immutable inputs.
    /// `content` is the full raw source bytes consumed by the operation.
    pub fn derive_id(&self, content: &[u8]) -> Result<CommitId, CommitError> {
        let salt = salt_bytes(&self.salt)?;
        Ok(id::derive_commit_id(
            &salt,
            &self.previous_id,
            &self.timestamp,
            content,
            &self.canonical_payload(),
        ))
    }

    /// Validate projections and shape before writing.
    /// `is_first` must be true only for the first record on the node's chain.
    pub fn validate(&self, is_first: bool) -> Result<(), CommitError> {
        salt_bytes(&self.salt)?;
        parse_timestamp(&self.timestamp)?;
        if is_first && !self.previous_id.is_empty() {
            return Err(CommitError::BadFirstPrevious);
        }
        // content_ref: "empty" or a 128-bit version id's hex.
        if self.content_ref != "empty" && Id128::from_hex(&self.content_ref).is_err() {
            return Err(CommitError::Projection("content_ref"));
        }
        // Closed payload: reject keys not allowed for this kind.
        let allowed = Self::allowed_payload_keys(self.kind);
        for k in self.payload.keys() {
            if !allowed.contains(k.as_str()) {
                return Err(CommitError::UnknownField(k.clone()));
            }
        }
        // Required keys per kind.
        for &req in Self::required_payload_keys(self.kind) {
            if !self.payload.contains_key(req) {
                return Err(CommitError::MissingField(req));
            }
        }
        Ok(())
    }

    /// Fields each kind permits inside `payload`.
    fn allowed_payload_keys(kind: CommitKind) -> BTreeSet<&'static str> {
        use CommitKind::*;
        match kind {
            Init => ["path"].into_iter().collect(),
            Commit | Clean | Unclean | FileVerify => {
                ["path", "range", "reason", "no_reason", "expected", "link_id", "changes", "stop"]
                    .into_iter()
                    .collect()
            }
            AtomicBegin | AtomicEnd => ["path", "chain"].into_iter().collect(),
            Link => ["path", "link_id", "source", "target", "reason"].into_iter().collect(),
            Adapt => ["path", "link_id", "changes", "reason", "stop"].into_iter().collect(),
            Rename => ["path", "source", "target", "reason"].into_iter().collect(),
            Delete => ["path", "source", "reason"].into_iter().collect(),
            Import | Remove => ["path", "include", "exclude", "scope"].into_iter().collect(),
            ScopeAdjust => ["path", "include", "exclude", "rule", "level", "skip"].into_iter().collect(),
            Tag => ["path", "tag"].into_iter().collect(),
            Reset => ["path", "requested", "actual", "warning"].into_iter().collect(),
        }
    }

    /// Fields each kind requires inside `payload`.
    fn required_payload_keys(kind: CommitKind) -> &'static [&'static str] {
        use CommitKind::*;
        match kind {
            Init | Import | Remove => &["path"],
            Link => &["link_id", "source", "target"],
            Adapt => &["link_id", "changes", "reason"],
            Rename => &["source", "target"],
            Delete => &["source"],
            Tag => &["path", "tag"],
            Reset => &["requested", "actual"],
            AtomicBegin | AtomicEnd => &[],
            Commit | Clean | Unclean | FileVerify | ScopeAdjust => &[],
        }
    }
}

pub fn kind_name(k: CommitKind) -> &'static str {
    use CommitKind::*;
    match k {
        Init => "init",
        Commit => "commit",
        Clean => "clean",
        Unclean => "unclean",
        AtomicBegin => "atomic_begin",
        AtomicEnd => "atomic_end",
        Link => "link",
        Adapt => "adapt",
        Rename => "rename",
        Delete => "delete",
        Import => "import",
        Remove => "remove",
        ScopeAdjust => "scope_adjust",
        Tag => "tag",
        FileVerify => "file_verify",
        Reset => "reset",
    }
}

fn parse_content_ref(s: &str) -> ContentRef {
    if s == "empty" {
        ContentRef::Empty
    } else {
        ContentRef::Version(Id128::from_hex(s).unwrap_or(Id128([0; 16])))
    }
}

fn salt_bytes(s: &str) -> Result<[u8; 16], CommitError> {
    if s.len() != 16 || !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(CommitError::BadSalt);
    }
    let mut a = [0u8; 16];
    a.copy_from_slice(s.as_bytes());
    Ok(a)
}

fn parse_timestamp(s: &str) -> Result<(), CommitError> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|_| ())
        .map_err(|_| CommitError::BadTimestamp)
}
