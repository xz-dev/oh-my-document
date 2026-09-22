//! Same-unit union coverage over source coordinates.
//!
//! Text excludes only Unicode `char::is_whitespace` positions from counting;
//! byte coverage counts every raw byte. Coordinates and gaps always stay in
//! original source units.

use std::collections::BTreeSet;

use crate::relations::range::{Mode, Range};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageStats {
    pub covered: u64,
    pub total: u64,
    pub gaps: Vec<Range>,
}

/// Compute union coverage for decoded text or UTF-8 bytes retained by legacy
/// library callers. CLI byte coverage uses [`byte_coverage`] with raw length.
pub fn coverage(ranges: &[Range], content: &str, mode: Mode) -> (u64, u64) {
    let stats = match mode {
        Mode::Text => text_coverage(ranges, content),
        Mode::Byte => byte_coverage(ranges, content.len() as u64),
    };
    (stats.covered, stats.total)
}

pub fn text_coverage(ranges: &[Range], content: &str) -> CoverageStats {
    let covered: BTreeSet<u64> = ranges
        .iter()
        .filter(|range| range.mode == Mode::Text)
        .flat_map(|range| range.start..range.end)
        .collect();
    let counted: Vec<u64> = content
        .chars()
        .enumerate()
        .filter_map(|(index, ch)| (!ch.is_whitespace()).then_some(index as u64))
        .collect();
    let uncovered: Vec<u64> = counted
        .iter()
        .copied()
        .filter(|position| !covered.contains(position))
        .collect();
    CoverageStats {
        covered: counted.len() as u64 - uncovered.len() as u64,
        total: counted.len() as u64,
        gaps: positions_to_gaps(&uncovered, Mode::Text),
    }
}

pub fn byte_coverage(ranges: &[Range], byte_len: u64) -> CoverageStats {
    let mut covered = BTreeSet::new();
    for range in ranges.iter().filter(|range| range.mode == Mode::Byte) {
        covered.extend(range.start..range.end.min(byte_len));
    }
    let uncovered: Vec<u64> = (0..byte_len)
        .filter(|position| !covered.contains(position))
        .collect();
    CoverageStats {
        covered: byte_len - uncovered.len() as u64,
        total: byte_len,
        gaps: positions_to_gaps(&uncovered, Mode::Byte),
    }
}

fn positions_to_gaps(positions: &[u64], mode: Mode) -> Vec<Range> {
    let mut gaps = Vec::new();
    let Some(&first) = positions.first() else {
        return gaps;
    };
    let mut start = first;
    let mut previous = first;
    for &position in &positions[1..] {
        if position != previous + 1 {
            gaps.push(Range {
                start,
                end: previous + 1,
                mode,
            });
            start = position;
        }
        previous = position;
    }
    gaps.push(Range {
        start,
        end: previous + 1,
        mode,
    });
    gaps
}

pub fn coverage_percent(covered: u64, denom: u64) -> f64 {
    if denom == 0 {
        100.0
    } else {
        covered as f64 * 100.0 / denom as f64
    }
}

pub fn effective_link_span(r: &Range, target_empty: bool) -> u64 {
    if target_empty || r.is_empty() {
        0
    } else {
        r.len()
    }
}
