//! Typed operation payloads — the domain meaning behind `Commit.payload`.
//!
//! A flat `serde_json::Map` carries the on-disk bytes, but the domain rules
//! (link instances, reset markers, adapt obligations, end-adjacent ranges)
//! need a typed view. These structs parse the payload into meaning and
//! enforce the closed-per-kind field set at the domain boundary.

use serde::{Deserialize, Serialize};

use crate::records::ids::Id128;
use crate::relations::range::{Mode, Range};

/// A link instance: a directed edge from one range-commit to another.
/// Persistent 128-bit `link_id` so identical direction/target duplicates can
/// coexist and be individually adapted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub link_id: Id128,
    /// Source range-commit endpoint (`file:path@start-end:commit_id`).
    pub source: String,
    /// Target range-commit endpoint.
    pub target: String,
    /// Direction is part of identity — a reverse query never creates the
    /// reverse link.
    pub reason: String,
}

/// An adaptation obligation: which changes on a link the caller handled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adapt {
    pub link_id: Id128,
    /// The changes being adapted (free-form selection string, e.g. a hunk id
    /// list); adaptation requires explicit link_id + changes + reason.
    pub changes: String,
    pub reason: String,
    /// `--stop`: source-end blocks further obligation propagation.
    pub stop: bool,
}

/// Reset marker fields: where the caller asked to land vs where it landed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetTarget {
    /// The requested target commit id.
    pub requested: String,
    /// The commit actually landed on ("" = withdrawn to nothing).
    pub actual: String,
    /// Non-empty when landing differed from request (JSON warning surface).
    pub warning: String,
}

/// An end-adjacent range note: a range that became dirty because an
/// insertion landed exactly at its end — dirty but NOT auto-expanded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EndAdjacent {
    pub range: Range,
    pub insertion_pos: u64,
}

/// The tracked object a range-commit addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeTarget {
    /// Parent file node key.
    pub file: String,
    /// Character/byte half-open span.
    pub range: Range,
    /// Coordinate mode carried into identity.
    pub mode: Mode,
}
