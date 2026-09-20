//! Source references: `proj:A:path`, `proj:root:path`, `proj:A:byte::path`,
//! `command::<exe>::<JSON args>`.
//!
//! Parse by *fixed prefix first*: `proj:` consumes the whole rest as the
//! path (never split on `::` or `/`); `byte::` marks byte-offset mode;
//! `command::` hands its argv tail to the JSON parser. Anything a string
//! can't express unambiguously goes through `--source-json`, never a
//! template language.

use super::command::parse_command_ref;
use super::SourceError;

/// A resolved source reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRef {
    /// `proj:<alias>:<path>` — a file in an aliased project (`root` = self).
    File { alias: String, path: String, byte: bool },
    /// `command::<exe>::<args>` — virtual file from program stdout.
    Command { executable: String, args: Vec<String> },
}

/// Parse a source reference string.
/// `byte` marks byte-offset coordinates (no decoding ever).
pub fn parse_source_ref(s: &str) -> Result<SourceRef, SourceError> {
    if let Some(rest) = s.strip_prefix("proj:") {
        // proj:<alias>:<rest> — rest keeps everything, including `::` and `/`.
        let (alias, path) = rest.split_once(':').ok_or(SourceError::Command)?;
        let (byte, path) = if let Some(p) = path.strip_prefix("byte::") {
            (true, p)
        } else {
            (false, path)
        };
        return Ok(SourceRef::File {
            alias: alias.to_string(),
            path: path.to_string(),
            byte,
        });
    }
    if s.starts_with("command::") {
        let (exe, args) = parse_command_ref(s)?;
        return Ok(SourceRef::Command { executable: exe, args });
    }
    Err(SourceError::Command)
}
