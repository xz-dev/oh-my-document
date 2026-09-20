//! Node keys: how file and range nodes are named in the mount tree.
//!
//! A range node is a *first-class object* with its own commit chain, mounted
//! under its file node. The key encodes the tracked span so the same file
//! can carry many independent ranges (including identical or overlapping
//! spans — those are separate chains with separate histories).

use crate::relations::range::Mode;

/// Key for a file node: `file:<path>`.
pub fn file_key(path: &str) -> String {
    format!("file:{path}")
}

/// Key for a range node: `range:<path>@<mode>:<start>-<end>` for the
/// canonical whole-span chain, or `range:<path>@<mode>:<start>-<end>#<nonce>`
/// for a duplicate-span independent chain. The span is part of identity; a
/// nonce distinguishes two tracked objects over identical coordinates.
pub fn range_key(path: &str, mode: Mode, start: u64, end: u64) -> String {
    let m = match mode {
        Mode::Text => "text",
        Mode::Byte => "byte",
    };
    format!("range:{path}@{m}:{start}-{end}")
}

/// A duplicate-coordinate independent range chain (`#nonce` suffix).
pub fn range_key_nonce(path: &str, mode: Mode, start: u64, end: u64, nonce: &str) -> String {
    format!("{}#{}", range_key(path, mode, start, end), nonce)
}

/// Parse a `--range` arg like `0-5` or `byte:0-5` into (mode, start, end).
/// Returns None when unparseable — caller reports the diagnostic, never a
/// silent clamp or a guessed span.
pub fn parse_range_arg(arg: &str) -> Option<(Mode, u64, u64)> {
    let (mode, rest) = if let Some(r) = arg.strip_prefix("byte:") {
        (Mode::Byte, r)
    } else if let Some(r) = arg.strip_prefix("text:") {
        (Mode::Text, r)
    } else {
        (Mode::Text, arg) // bare "0-5" defaults to text coordinates
    };
    let (s, e) = rest.split_once('-')?;
    let start = s.trim().parse().ok()?;
    let end = e.trim().parse().ok()?;
    Some((mode, start, end))
}

/// Does `key` name a range node (vs a file node)?
pub fn is_range_key(key: &str) -> bool {
    key.starts_with("range:")
}

/// The file a range node mounts under.
pub fn parent_of(key: &str) -> String {
    if let Some(rest) = key.strip_prefix("range:") {
        let path = rest.split('@').next().unwrap_or("");
        file_key(path)
    } else {
        "root".to_string()
    }
}

/// Is `key` a whole-file endpoint (illegal as a link endpoint)?
pub fn is_file_key(key: &str) -> bool {
    key.starts_with("file:")
}
