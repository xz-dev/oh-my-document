//! File sources: observe current content at a registered path.
//!
//! Reads the live file — never Git HEAD, never an index. The bytes are what
//! the file contains *now*, which is what `check` reports on and what a new
//! commit captures.

use std::path::Path;

use super::{observe_file, Observation, SourceError};

/// Observe a text file under the chosen encoding.
pub fn observe_text(path: &Path, encoding: Option<&str>) -> Result<Observation, SourceError> {
    observe_file(path, true, encoding)
}

/// Observe a file in byte mode — no decode.
pub fn observe_bytes(path: &Path) -> Result<Observation, SourceError> {
    observe_file(path, false, None)
}
