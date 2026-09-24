//! Relations: the parent→child mount tree and per-node commit chain.
//!
//! File and range nodes are mounted under a parent (project root for files,
//! file for ranges). Each node keeps an append-only chain of commits linked
//! by `previous_id`; the node's *tip* is the newest commit on that chain.
//! Reset moves the tip pointer; it never deletes chain entries.

pub mod atomic;
pub mod classify;
pub mod coverage;
pub mod diff;
pub mod dirty;
pub mod identity;
pub mod linkhealth;
pub mod node;
pub mod range;

use serde::{Deserialize, Serialize};

/// A node in the mount tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Chain key identifying this node's commit sequence (e.g. `file:docs/a.md`,
    /// `range:docs/a.md@1-40`).
    pub key: String,
    /// Node kind.
    pub kind: NodeKind,
    /// Parent node key (`""` for the project root).
    pub parent: String,
    /// Tip commit id on this node's chain ("" if no commits yet).
    pub tip: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Invisible project root that file nodes mount under.
    Root,
    /// A file tracked as a whole.
    File,
    /// A tracked range inside a file (text coords or byte coords).
    Range,
}

/// Follow `previous_id` links from `tip` back to genesis.
/// Returns commit ids in tip→root order. `lookup` resolves a commit id to
/// its `previous_id`; returns None for missing records.
pub fn chain_to_root<F>(tip: &str, mut prev_of: F) -> Vec<String>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = Vec::new();
    let mut cur = tip.to_string();
    while !cur.is_empty() {
        out.push(cur.clone());
        match prev_of(&cur) {
            Some(p) => cur = p,
            None => break,
        }
    }
    out
}

/// File-commit `range_id -> tip` snapshot semantics: a file reset restores
/// each range's recorded tip exactly, not by wall-clock.
pub fn restore_range_tips(
    snapshot: &std::collections::BTreeMap<String, String>,
) -> Vec<(String, String)> {
    snapshot
        .iter()
        .map(|(r, t)| (r.clone(), t.clone()))
        .collect()
}
pub mod tags;
