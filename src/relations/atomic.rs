//! ATOMIC blocks and reset (design E-4/E-5).
//!
//! `commit atomic_begin` / `commit atomic_end` are placeholder marker
//! commits. They form a stack on a single node chain: each END closes only
//! the nearest unclosed BEGIN (innermost-first), never a sibling's.
//!
//! Reset semantics:
//!   * reset to a *marker* (BEGIN/END) withdraws the marker and its
//!     successors, landing on the marker's direct previous_id predecessor —
//!     the "-1 step" the design calls out for placeholder commits.
//!   * reset to an ordinary interior member is refused — only markers are
//!     reset targets.
//!   * nested blocks are allowed; resetting a file into a child ordinary
//!     member rejects the whole reset.
//!   * an open (unclosed) block fails `verify` even if coverage is 100% —
//!     the block must close before the chain is considered clean.

use std::collections::VecDeque;

use serde::Serialize;

use crate::records::commit::CommitKind;

/// A marker on a node chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub commit_id: String,
    pub kind: CommitKind, // AtomicBegin or AtomicEnd
    /// previous_id of the marker — where a reset to it would land.
    pub previous: String,
}

/// Per-node open-block stack (innermost last).
#[derive(Debug, Default)]
pub struct AtomicStack {
    /// Unclosed BEGINs, outermost-first.
    pub open: VecDeque<String>,
}

impl AtomicStack {
    /// Push a BEGIN marker.
    pub fn begin(&mut self, commit_id: &str) {
        self.open.push_back(commit_id.to_string());
    }

    /// Close the nearest open BEGIN with an END. Returns the BEGIN it
    /// closed, or None if none open (an END with no matching BEGIN is a
    /// structural error the caller reports).
    pub fn end(&mut self) -> Option<String> {
        self.open.pop_back()
    }

    /// Any unclosed BEGIN? `verify` fails while this is non-empty.
    pub fn is_open(&self) -> bool {
        !self.open.is_empty()
    }
}

/// What a reset to `target` resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetLanding {
    /// Reset to a marker: land on the marker's direct predecessor
    /// ("" = withdraw to nothing — the first BEGIN with no predecessor).
    MarkerPredecessor(String),
    /// Reset to an ordinary interior member — refused.
    RefusedInterior,
    /// Reset into a child ordinary member (file reset crossing a block
    /// member) — the whole reset rejects, siblings unchanged.
    RefusedIntoChildMember,
}

/// Resolve where a reset to `target` lands.
/// `target_kind` is the kind of commit `target` names; `target_prev` its
/// previous_id. `in_child_member` is true when the reset would descend into
/// a child ordinary member inside an open/closed block.
pub fn resolve_reset(
    target_kind: CommitKind,
    target_prev: &str,
    in_child_member: bool,
) -> ResetLanding {
    if in_child_member {
        return ResetLanding::RefusedIntoChildMember;
    }
    match target_kind {
        CommitKind::AtomicBegin | CommitKind::AtomicEnd => {
            ResetLanding::MarkerPredecessor(target_prev.to_string())
        }
        _ => ResetLanding::RefusedInterior,
    }
}

/// Serialize a reset result for the `--json` envelope (E-12.4): requested
/// vs actual landing plus a machine-readable warning when they differ.
#[derive(Debug, Serialize)]
pub struct ResetReport {
    pub requested: String,
    pub actual: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub warning: String,
}
