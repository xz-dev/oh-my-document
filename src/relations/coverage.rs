//! Coverage: union-counted link coverage over content positions (E-6).
//!
//! Coverage counts *positions covered by at least one confirmed link* over
//! the source's denominator — never by summing per-file or per-link ratios.
//! Overlapping links never double-count; unmarked content stays in the
//! denominator. Text sources filter Unicode White_Space out of the counted
//! positions (so whitespace edits don't fake coverage); byte sources count
//! raw offsets with no filtering.

use std::collections::BTreeSet;

use crate::relations::range::{Mode, Range};

/// Compute union-coverage: the set of positions covered by ≥1 link.
/// Returns (covered_positions, denominator) in the source's own unit.
///
/// `ranges` are confirmed-link spans. `content` is the decoded text (text
/// mode) or ignored (byte mode). Text denominator filters White_Space so
/// whitespace-only positions don't inflate or fake coverage.
pub fn coverage(ranges: &[Range], content: &str, mode: Mode) -> (u64, u64) {
    match mode {
        Mode::Text => {
            // Denominator: non-whitespace char positions.
            let mut covered = BTreeSet::new();
            let mut denom = 0u64;
            for (i, ch) in content.chars().enumerate() {
                if ch.is_whitespace() {
                    continue;
                }
                denom += 1;
                let pos = i as u64;
                for r in ranges {
                    if r.mode == Mode::Text && pos >= r.start && pos < r.end {
                        covered.insert(pos);
                        break;
                    }
                }
            }
            (covered.len() as u64, denom)
        }
        Mode::Byte => {
            // Byte mode: no whitespace filtering; denominator = byte length.
            let denom = content.len() as u64;
            let mut covered = BTreeSet::new();
            for r in ranges {
                if r.mode == Mode::Byte {
                    for pos in r.start..r.end.min(denom) {
                        covered.insert(pos);
                    }
                }
            }
            (covered.len() as u64, denom)
        }
    }
}

/// Empty or all-whitespace text reports 100% (logical N/A shown as 100% so
/// the display isn't ragged), per the spec.
pub fn coverage_percent(covered: u64, denom: u64) -> f64 {
    if denom == 0 {
        100.0
    } else {
        covered as f64 * 100.0 / denom as f64
    }
}

/// A range linked to an *empty* target fills no gap — it covers nothing.
pub fn effective_link_span(r: &Range, target_empty: bool) -> u64 {
    if target_empty || r.is_empty() {
        0
    } else {
        r.len()
    }
}
