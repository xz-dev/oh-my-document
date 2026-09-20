//! Range coordinates: 0-based, left-closed right-open positions over a
//! decoded text view or raw bytes.
//!
//! Text ranges count Unicode scalar positions in the decoded view — never
//! bytes, never UTF-16 units. Byte ranges count raw offsets and never
//! decode. The mode is part of the range's identity: the same source under
//! byte vs text mode is a different tracked object.

use serde::{Deserialize, Serialize};

/// Coordinate mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Unicode scalar positions over the decoded view.
    Text,
    /// Raw byte offsets; no decoding ever.
    Byte,
}

/// A half-open `[start, end)` range. `start` is a position index (char or
/// byte depending on mode); `end` is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: u64,
    pub end: u64,
    pub mode: Mode,
}

#[derive(Debug, thiserror::Error)]
pub enum RangeError {
    #[error("end before start")]
    Inverted,
    #[error("range out of bounds")]
    OutOfBounds,
    #[error("invalid decode under chosen encoding")]
    Decode,
}

impl Range {
    /// Construct a validated range. `len` is the source's extent in the same
    /// unit (chars for text, bytes for byte mode).
    pub fn new(start: u64, end: u64, mode: Mode, len: u64) -> Result<Self, RangeError> {
        if end < start {
            return Err(RangeError::Inverted);
        }
        if end > len {
            return Err(RangeError::OutOfBounds);
        }
        Ok(Self { start, end, mode })
    }

    /// Whole-source range.
    pub fn whole(mode: Mode, len: u64) -> Self {
        Self { start: 0, end: len, mode }
    }

    /// Length in the range's own unit.
    pub fn len(&self) -> u64 {
        self.end - self.start
    }

    /// An empty range (start == end): tracked, contributes no coverage.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Overlap: two ranges in the same mode intersect iff their half-open
    /// intervals share at least one position.
    pub fn overlaps(&self, other: &Range) -> bool {
        self.mode == other.mode && self.start < other.end && other.start < self.end
    }
}

/// Count Unicode scalar positions in a decoded text (the unit text ranges
/// use). BOM and CRLF are real characters and count.
pub fn text_len(s: &str) -> u64 {
    s.chars().count() as u64
}

/// Extract the `[start, end)` char slice of a decoded text.
/// Returns the substring; out-of-bounds is an error, never a clamp.
pub fn text_slice<'a>(s: &'a str, r: &Range) -> Result<&'a str, RangeError> {
    if r.mode != Mode::Text {
        return Err(RangeError::Decode);
    }
    let mut start_byte = None;
    let mut end_byte = None;
    for (i, (b, _)) in s.char_indices().enumerate() {
        if i as u64 == r.start {
            start_byte = Some(b);
        }
        if i as u64 == r.end {
            end_byte = Some(b);
            break;
        }
    }
    if r.start == s.chars().count() as u64 {
        start_byte = Some(s.len());
    }
    if r.end == s.chars().count() as u64 && end_byte.is_none() {
        end_byte = Some(s.len());
    }
    match (start_byte, end_byte) {
        (Some(a), Some(b)) => Ok(&s[a..b]),
        (Some(a), None) if r.end == s.chars().count() as u64 => Ok(&s[a..]),
        _ => Err(RangeError::OutOfBounds),
    }
}
