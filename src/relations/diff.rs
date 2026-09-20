//! Diff: full old-vs-new content comparison via Myers, plus candidate
//! migration for tracked ranges (design E-4).
//!
//! The comparison always runs over the *complete* old and new source
//! contents — never a diff reused from Git or an index. Line-level Myers
//! decides which characters/bytes changed; a tracked range touching a
//! changed region becomes dirty. An insertion adjacent to a range's end is
//! ambiguous ("cannot prove unrelated") — it dirties that range without
//! expanding it or auto-confirming the new text.

use similar::{ChangeTag, TextDiff};

use crate::relations::range::Range;

/// A changed hunk: `[pos, pos+len)` in new-content coordinates that differs
/// from old, plus whether it is an insertion adjacent to a range end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    /// Start position in new-content units.
    pub pos: u64,
    /// Length in new-content units (0 for pure deletion).
    pub len: u64,
    /// True when this hunk is a pure insertion (no old content removed).
    pub insertion: bool,
}

/// Diff old vs new *complete* contents, returning changed hunks in
/// new-content character units. Operates on the full decoded text — never
/// on a pre-existing diff.
pub fn diff_text(old: &str, new: &str) -> Vec<Hunk> {
    let diff = TextDiff::from_chars(old, new);
    let mut hunks = Vec::new();
    let mut pos = 0u64;
    let mut pending: Option<Hunk> = None;
    for change in diff.iter_all_changes() {
        let v = change.value();
        let n = v.chars().count() as u64;
        match change.tag() {
            ChangeTag::Equal => {
                if let Some(h) = pending.take() {
                    hunks.push(h);
                }
                pos += n;
            }
            ChangeTag::Delete => {
                // Deletion consumes old, not new positions.
                let h = pending.get_or_insert(Hunk { pos, len: 0, insertion: true });
                h.insertion = h.insertion && h.len == 0;
                // deletion itself doesn't advance pos
            }
            ChangeTag::Insert => {
                let h = pending.get_or_insert(Hunk { pos, len: 0, insertion: true });
                h.len += n;
                pos += n;
            }
        }
    }
    if let Some(h) = pending {
        hunks.push(h);
    }
    hunks
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
