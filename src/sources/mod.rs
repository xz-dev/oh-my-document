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
pub mod git;
pub mod permission;
pub mod projects;
pub mod reference;
pub mod scope;

use std::path::Path;

pub use reference::SourceDescriptor;

/// Normalize persisted source descriptors at the owning project boundary.
/// File inputs may be absolute locally, but shared records never are.
pub fn normalize_descriptor(
    descriptor: SourceDescriptor,
    config_cwd: &Path,
    owning_project_root: &Path,
) -> Result<SourceDescriptor, SourceError> {
    match descriptor {
        SourceDescriptor::File { project, path } => {
            let (path, _) =
                projects::resolve_path(config_cwd, owning_project_root, &project, &path)
                    .map_err(|error| SourceError::Invalid(error.to_string()))?;
            Ok(SourceDescriptor::File { project, path })
        }
        SourceDescriptor::Git {
            project,
            commit,
            path,
        } => {
            projects::resolve(config_cwd, owning_project_root, &project, "")
                .map_err(|error| SourceError::Invalid(error.to_string()))?;
            let path = git::normalize_repo_path(Path::new(&path))?;
            Ok(SourceDescriptor::Git {
                project,
                commit,
                path,
            })
        }
        other => Ok(other),
    }
}

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
    #[error("invalid source fields: {0}")]
    Invalid(String),
    #[error("command failed (non-zero exit){0}")]
    CommandFailed(String),
    #[error("source unavailable: {0}")]
    Unavailable(String),
}

/// Read the file at its registered path *right now*.
/// `text` selects decoded-text semantics; `encoding` is used when `text`.
/// Byte mode never decodes.
pub fn observe_file(
    path: &Path,
    text: bool,
    encoding: Option<&str>,
) -> Result<Observation, SourceError> {
    let bytes = std::fs::read(path)?;
    if text {
        let enc = encoding.unwrap_or("utf-8");
        // Decode-check now: invalid text under the chosen encoding is a
        // source error, not silently stored as byte garbage.
        decode(&bytes, enc)?;
        Ok(Observation {
            bytes,
            text: true,
            encoding: Some(enc.to_string()),
        })
    } else {
        Ok(Observation {
            bytes,
            text: false,
            encoding: None,
        })
    }
}

/// Decode bytes under a named encoding via `encoding_rs` (WHATWG labels:
/// utf-8, utf-16le/be, latin1/iso-8859-*, windows-125*, shift_jis, gbk,
/// big5, euc-jp/kr, koi8-r, …). An unknown label or undecodable input is an
/// error, never a silent fallback — a recorded encoding freezes that
/// observation, never reinterprets history.
pub(crate) fn decode(bytes: &[u8], encoding: &str) -> Result<String, SourceError> {
    let enc = encoding_rs::Encoding::for_label(encoding.trim().as_bytes())
        .ok_or(SourceError::Encoding)?;
    // decode_without_bom_handling_and_without_replacement returns None on
    // malformed input — invalid text under the chosen encoding is a source
    // error, not silently stored garbage.
    enc.decode_without_bom_handling_and_without_replacement(bytes)
        .map(|cow| cow.into_owned())
        .ok_or(SourceError::Encoding)
}

/// Collect one complete source exactly once. Project aliases resolve through
/// machine-local mappings; command cwd is always the owning project root.
pub fn collect(
    descriptor: &SourceDescriptor,
    config_cwd: &Path,
    owning_project_root: &Path,
    text: bool,
    encoding: Option<&str>,
) -> Result<Observation, SourceError> {
    descriptor.validate()?;
    let selected_encoding = if text {
        let encoding = encoding.unwrap_or("utf-8");
        encoding::validate(encoding)?;
        Some(encoding)
    } else {
        None
    };
    let bytes = match descriptor {
        SourceDescriptor::File { project, path } => {
            let resolved = projects::resolve(config_cwd, owning_project_root, project, path)
                .map_err(|error| SourceError::Invalid(error.to_string()))?;
            std::fs::read(resolved)?
        }
        SourceDescriptor::Command { executable, args } => {
            let outcome = command::observe_command(executable, args, owning_project_root)?;
            if !outcome.exit_ok {
                let stderr = String::from_utf8_lossy(&outcome.stderr);
                let suffix = if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {}", stderr.trim_end())
                };
                return Err(SourceError::CommandFailed(suffix));
            }
            outcome.stdout
        }
        SourceDescriptor::Git {
            project,
            commit,
            path,
        } => {
            let repo = projects::resolve(config_cwd, owning_project_root, project, "")
                .map_err(|error| SourceError::Invalid(error.to_string()))?;
            git::read_blob(&git::GitRef {
                repo,
                commit: commit.clone(),
                path: path.clone(),
            })?
        }
    };
    if let Some(encoding) = selected_encoding {
        decode(&bytes, encoding)?;
        Ok(Observation {
            bytes,
            text: true,
            encoding: Some(encoding.to_string()),
        })
    } else {
        Ok(Observation {
            bytes,
            text: false,
            encoding: None,
        })
    }
}
