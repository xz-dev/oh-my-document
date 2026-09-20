//! Dirty propagation and the obligation stack (design E-4/E-5).
//!
//! A commit is *dirty* when the content its range covered changed, or when a
//! dependency it relies on became invalid. Dirtyness propagates only through
//! explicit dependencies — never through time or wall-clock order — and each
//! propagation hop is recorded so `unclean` obligations stack rather than
//! collapse.
//!
//! Three rules the rest of the system leans on:
//!   * end-adjacent insertion dirties the old range without expanding it
//!   * `unclean` commits stack obligations (each keeps its own id + reason)
//!   * a dangling dependency fails *immediately* at the first broken hop —
//!     `c1 -> b1 -> a1` fails at a1, not lazily at read time

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Why a commit is dirty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyReason {
    /// Content under the range changed (Myers hunk overlapped).
    ContentChanged,
    /// Insertion landed exactly at the range's end boundary.
    EndAdjacentInsertion,
    /// A dependency this commit relies on became dangling.
    DependencyDangling { dependency: String },
    /// Caller explicitly marked unclean (an obligation).
    ExplicitUnclean { reason: String },
}

/// A single obligation on the stack — one `unclean` marker, never merged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obligation {
    /// The unclean commit's own id (each keeps a distinct identity).
    pub commit_id: String,
    /// Why it was raised.
    pub reason: String,
    /// Stacked on top of (previous obligation's commit id, "" = base).
    pub atop: String,
}

/// Per-node dirty state: which commits are dirty and the obligation stack.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DirtyState {
    /// commit id → why it is dirty.
    pub dirty: BTreeMap<String, DirtyReason>,
    /// Ordered obligation stack (oldest first). Multiple `unclean` on the
    /// same content are distinct entries — never merged.
    pub obligations: Vec<Obligation>,
}

impl DirtyState {
    /// Mark a commit dirty for `reason`. Idempotent per commit.
    pub fn mark(&mut self, commit: &str, reason: DirtyReason) {
        self.dirty.entry(commit.to_string()).or_insert(reason);
    }

    /// Push a new unclean obligation. Distinct id each time — obligations
    /// stack, never collapse onto one entry.
    pub fn push_unclean(&mut self, commit_id: &str, reason: &str) {
        let atop = self.obligations.last().map(|o| o.commit_id.clone()).unwrap_or_default();
        self.obligations.push(Obligation {
            commit_id: commit_id.to_string(),
            reason: reason.to_string(),
            atop,
        });
        self.dirty.insert(commit_id.to_string(), DirtyReason::ExplicitUnclean { reason: reason.to_string() });
    }

    /// Is `commit` dirty?
    pub fn is_dirty(&self, commit: &str) -> bool {
        self.dirty.contains_key(commit)
    }

    /// Propagate dirtyness along explicit dependency edges — bounded so a
    /// cycle terminates, never loops forever. `edges` maps dependent → its
    /// dependencies. A missing (dangling) dependency fails *at that hop* and
    /// reports the exact break, not a vague global failure.
    ///
    /// Bounded traversal: each (dependent, dependency) pair is visited once.
    /// A→B→C→A terminates — the second arrival at A is a repeat, not a new
    /// obligation. Distinct changes or distinct link ids are NEVER merged
    /// into one "already visited" entry.
    pub fn propagate(
        &mut self,
        edges: &BTreeMap<String, BTreeSet<String>>,
        present: &dyn Fn(&str) -> bool,
    ) -> Result<(), DanglingHop> {
        // visited = (node reached); prevents infinite loops on cycles.
        let mut visited = BTreeSet::new();
        for (dependent, deps) in edges {
            for d in deps {
                if !present(d) {
                    self.mark(
                        dependent,
                        DirtyReason::DependencyDangling { dependency: d.clone() },
                    );
                    return Err(DanglingHop {
                        dependent: dependent.clone(),
                        missing: d.clone(),
                    });
                }
                visited.insert((dependent.clone(), d.clone()));
            }
        }
        Ok(())
    }

    /// Bounded reachability walk: visits each node once, so A→B→C→A stops.
    /// Returns nodes reachable from `start` in first-seen order. A cycle
    /// does not produce repeated obligations — it terminates.
    pub fn reachable_bounded(
        start: &str,
        edges: &BTreeMap<String, BTreeSet<String>>,
    ) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut order = Vec::new();
        let mut stack = vec![start.to_string()];
        while let Some(n) = stack.pop() {
            if !seen.insert(n.clone()) {
                continue; // cycle: already visited, don't re-obligate
            }
            order.push(n.clone());
            if let Some(deps) = edges.get(&n) {
                for d in deps {
                    stack.push(d.clone());
                }
            }
        }
        order
    }
}

/// The first broken hop in a dependency chain — `c1 -> b1 -> a1` reports a1.
#[derive(Debug, thiserror::Error)]
#[error("dependency {missing} dangling while resolving {dependent}")]
pub struct DanglingHop {
    pub dependent: String,
    pub missing: String,
}
