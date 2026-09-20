//! Source semantics: how bytes are observed at the current path.
//!
//! File and command sources share identical lifecycle semantics — the only
//! difference is where the bytes come from (file read vs command stdout).
//! Observation always reads the registered path *now*, never Git HEAD or
//! an index snapshot. Decode happens only in text mode under a chosen
//! encoding; byte mode never decodes.

pub mod command;
pub mod discovery;
pub mod encoding;
pub mod file;
pub mod permission;
pub mod reference;

use std::path::Path;

/// A raw observation of a source at this moment.
#[derive(Debug)]
pub struct Observation {
    /// Full raw bytes (never truncated).
    pub bytes: Vec<u8>,
    /// Whether this is a text (decoded) or byte observation.
    pub text: bool,
    /// Encoding name when `text` is true (e.g. "utf-8").
    pub encoding: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid text encoding for this file")]
    Encoding,
    #[error("command failed (non-zero exit)")]
    Command,
}

/// Read the file at its registered path *right now*.
/// `text` selects decoded-text semantics; `encoding` is used when `text`.
/// Byte mode never decodes.
pub fn observe_file(path: &Path, text: bool, encoding: Option<&str>) -> Result<Observation, SourceError> {
    let bytes = std::fs::read(path)?;
    if text {
        let enc = encoding.unwrap_or("utf-8");
        // Decode-check now: invalid text under the chosen encoding is a
        // source error, not silently stored as byte garbage.
        decode(&bytes, enc)?;
        Ok(Observation { bytes, text: true, encoding: Some(enc.to_string()) })
    } else {
        Ok(Observation { bytes, text: false, encoding: None })
    }
}

/// Decode bytes under a named encoding. Only utf-8 and a small fixed set are
/// supported; unknown or undecodable input is an error, not a fallback.
fn decode(bytes: &[u8], encoding: &str) -> Result<String, SourceError> {
    match encoding.to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => String::from_utf8(bytes.to_vec()).map_err(|_| SourceError::Encoding),
        _ => Err(SourceError::Encoding),
    }
}
pub mod scope;
