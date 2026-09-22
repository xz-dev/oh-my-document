//! Diff: full old-vs-new content comparison via Myers, plus candidate
//! migration for tracked ranges (design E-4).
//!
//! The comparison always runs over the *complete* old and new source
//! contents — never a diff reused from Git or an index. Line-level Myers
//! decides which characters/bytes changed; a tracked range touching a
//! changed region becomes dirty. An insertion adjacent to a range's end is
//! ambiguous ("cannot prove unrelated") — it dirties that range without
//! expanding it or auto-confirming the new text.

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::relations::range::Range;

/// A changed hunk in recorded-content coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    /// Start position in old/recorded-content units.
    pub pos: u64,
    /// Affected recorded extent (insertions use their new extent).
    pub len: u64,
    /// True when this hunk is a pure insertion (no old content removed).
    pub insertion: bool,
}

/// Diff old vs new *complete* contents, returning changed hunks in
/// new-content character units. Operates on the full decoded text — never
/// on a pre-existing diff.
fn hunks_from_ops(ops: Vec<DiffOp>) -> Vec<Hunk> {
    ops.into_iter()
        .filter_map(|op| match op {
            DiffOp::Equal { .. } => None,
            DiffOp::Delete {
                old_index, old_len, ..
            } => Some(Hunk {
                pos: old_index as u64,
                len: old_len as u64,
                insertion: false,
            }),
            DiffOp::Insert {
                old_index, new_len, ..
            } => Some(Hunk {
                pos: old_index as u64,
                len: new_len as u64,
                insertion: true,
            }),
            DiffOp::Replace {
                old_index,
                old_len,
                new_len,
                ..
            } => Some(Hunk {
                pos: old_index as u64,
                len: old_len.max(new_len) as u64,
                insertion: false,
            }),
        })
        .collect()
}

pub fn diff_text(old: &str, new: &str) -> Vec<Hunk> {
    let old: Vec<char> = old.chars().collect();
    let new: Vec<char> = new.chars().collect();
    hunks_from_ops(capture_diff_slices(Algorithm::Myers, &old, &new))
}

/// Diff complete raw byte sequences with Myers, returning byte-coordinate
/// hunks. No decoding or whitespace treatment occurs on this path.
pub fn diff_bytes(old: &[u8], new: &[u8]) -> Vec<Hunk> {
    hunks_from_ops(capture_diff_slices(Algorithm::Myers, old, new))
}

/// Every raw-byte start where `fragment` occurs in `content`.
pub fn locate_byte_candidates(fragment: &[u8], content: &[u8]) -> Vec<usize> {
    if fragment.is_empty() || fragment.len() > content.len() {
        return Vec::new();
    }
    content
        .windows(fragment.len())
        .enumerate()
        .filter_map(|(i, window)| (window == fragment).then_some(i))
        .collect()
}

/// Locate-candidate diagnostics for a fragment that lost reliable placement.
///
/// When a tracked range can no longer be matched unambiguously, the spec
/// requires reporting old coordinates, the difference, and *candidate*
/// positions — never silently re-point the track at one same-text match or
/// keep a stale confirmation. `locate_candidates` returns every start index
/// where `fragment` occurs in `content`; >1 is ambiguous (a duplicate
/// fragment, not a chosen placement).
pub fn locate_candidates(fragment: &str, content: &str) -> Vec<usize> {
    if fragment.is_empty() {
        return vec![];
    }
    content
        .match_indices(fragment)
        .map(|(byte, _)| content[..byte].chars().count())
        .collect()
}

/// Is the fragment's placement ambiguous — more than one same-text match?
/// Ambiguity MUST surface candidates; the system MUST NOT pick one.
pub fn ambiguous_placement(fragment: &str, content: &str) -> bool {
    locate_candidates(fragment, content).len() > 1
}

/// Which tracked ranges does a hunk dirty?
///
/// A range is dirtied when a hunk *overlaps* it (change inside the range) or
/// when an *insertion* lands exactly at the range's end boundary — that case
/// cannot prove the new text is unrelated, so the range dirties but does NOT
/// grow to cover it.
pub fn dirtied_by(hunks: &[Hunk], ranges: &[Range]) -> Vec<bool> {
    ranges
        .iter()
        .map(|r| {
            hunks.iter().any(|h| {
                let overlaps = h.pos < r.end && (h.pos + h.len) > r.start;
                let end_adjacent_insert = h.insertion && h.pos == r.end;
                overlaps || end_adjacent_insert
            })
        })
        .collect()
}
