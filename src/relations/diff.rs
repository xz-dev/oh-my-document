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
    /// Old extent consumed by this hunk (0 for a pure insertion).
    pub old_len: u64,
    /// New extent produced by this hunk (0 for a pure deletion).
    pub new_len: u64,
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
                old_len: old_len as u64,
                new_len: 0,
            }),
            DiffOp::Insert {
                old_index, new_len, ..
            } => Some(Hunk {
                pos: old_index as u64,
                len: new_len as u64,
                insertion: true,
                old_len: 0,
                new_len: new_len as u64,
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
                old_len: old_len as u64,
                new_len: new_len as u64,
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

/// Map a position in old-content coordinates forward through `hunks` into
/// new-content coordinates. Hunks arrive in ascending `pos` order.
///
/// Boundary rule: an insertion landing exactly at `old_pos` is ambiguous —
/// does the new text belong before or inside the point? `is_start` decides:
/// for a range START the point maps *after* the inserted text (the old text
/// begins later); for a range END it stays put (the insertion is outside
/// the range — mirrors `dirtied_by`'s end-adjacent rule).
///
/// A position inside a hunk's old extent clamps to the hunk's new end
/// boundary — the mapped point stays at the nearest surviving boundary,
/// never invented mid-hunk.
pub fn map_position(hunks: &[Hunk], old_pos: u64, is_start: bool) -> u64 {
    let mut pos = old_pos;
    for h in hunks {
        let old_end = h.pos + h.old_len;
        if h.pos > old_pos {
            break; // strictly after — no shift
        }
        let new_end = h.pos + h.new_len;
        if old_pos < old_end && h.old_len > 0 {
            // inside the changed region — clamp to the hunk's new end
            return new_end;
        }
        if h.pos == old_pos && h.insertion && !is_start {
            break; // end-adjacent insertion: stays outside the range
        }
        // hunk before the point, or a boundary insertion pushing a start
        pos = (pos as i64 + (h.new_len as i64 - h.old_len as i64)) as u64;
    }
    pos
}

/// Map a `[start,end)` range through hunks, returning the shifted span.
/// Returns None when the mapping collapses (end <= start) — the caller
/// must then refuse to continue rather than guess.
pub fn map_range(hunks: &[Hunk], range: Range) -> Option<Range> {
    let start = map_position(hunks, range.start, true);
    let end = map_position(hunks, range.end, false);
    if end > start {
        Some(Range {
            start,
            end,
            ..range
        })
    } else {
        None
    }
}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relations::range::Mode;

    fn rng(s: u64, e: u64) -> Range {
        Range {
            start: s,
            end: e,
            mode: Mode::Text,
        }
    }

    /// rustfmt-style whitespace re-wrap: `let count = 3;` → `let count  =  3;`
    /// inserts a space at pos 21 and pos 24 — the range covering `count = 3`
    /// must shift right by the inserted spaces.
    #[test]
    fn cosmetic_rewrap_maps_range() {
        let old = "fn f() {\n    let count = 3;\n}";
        let new = "fn f() {\n    let count  =  3;\n}";
        let hunks = diff_text(old, new);
        // range covering `let count = 3` (chars 14..27 of old)
        let r = rng(14, 27);
        let mapped = map_range(&hunks, r).expect("should map");
        // new text at mapped span should contain the reformatted `count  =  3`
        let new_text: String = new.chars().collect::<Vec<_>>()
            [mapped.start as usize..mapped.end as usize]
            .iter()
            .collect();
        assert!(new_text.contains("count"), "mapped text: {new_text:?}");
        assert!(new_text.contains("3"), "mapped text: {new_text:?}");
        assert!(mapped.end > mapped.start);
    }

    /// A pure insertion before the range shifts it right by the insert len.
    #[test]
    fn insertion_before_range_shifts() {
        let old = "abcd";
        let new = "abXYZcd"; // insert XYZ at pos 2
        let hunks = diff_text(old, new);
        let mapped = map_range(&hunks, rng(2, 4)).expect("map");
        // `cd` moved from [2,4) to [5,7)
        assert_eq!((mapped.start, mapped.end), (5, 7));
    }

    /// A change entirely after the range leaves it unmoved.
    #[test]
    fn change_after_range_unmoved() {
        let old = "fn f() { 3 }";
        let new = "fn f() { 3 } extra";
        let hunks = diff_text(old, new);
        let mapped = map_range(&hunks, rng(0, 5)).expect("map");
        assert_eq!((mapped.start, mapped.end), (0, 5));
    }
}
