//! Command sources use a fixed executable plus a JSON string-array argv.
//!
//! A command object is a virtual file source whose bytes come from running a
//! fixed executable with a hard-coded literal argv — never a shell string,
//! never variable substitution, never argv re-split on `::` or spaces.
//! Only complete stdout from a normal `exit 0` is source content; stderr is
//! a separate diagnostic channel and partial output never advances the
//! source version. Execution runs in the owning project root.

use std::path::Path;
use std::process::Command;

use super::{Observation, SourceError};

/// Parse the independent `--args-json` value.
/// Arguments must be strings only; no coercion or shell re-splitting.
pub fn parse_args_json(raw: &str) -> Result<Vec<String>, SourceError> {
    let arr: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| SourceError::Invalid("--args-json must be a JSON string array".into()))?;
    let items = arr
        .as_array()
        .ok_or_else(|| SourceError::Invalid("--args-json must be a JSON string array".into()))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| SourceError::Invalid("--args-json must contain strings only".into()))
        })
        .collect()
}

/// Execute the command once in `project_root`, capturing complete stdout.
/// `stdin` is EOF. Environment/PATH are inherited. On non-zero exit the
/// result is a failure carrying whatever was captured — the caller keeps the
/// previous successful version; partial stdout never becomes content.
pub fn observe_command(
    exe: &str,
    argv: &[String],
    project_root: &Path,
) -> Result<CommandOutcome, SourceError> {
    let output = Command::new(exe)
        .args(argv)
        .current_dir(project_root)
        .stdin(std::process::Stdio::null())
        .output()?;
    let ok = output.status.success();
    Ok(CommandOutcome {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_ok: ok,
    })
}

/// What one command execution produced.
#[derive(Debug)]
pub struct CommandOutcome {
    /// Complete stdout bytes (content only when `exit_ok`).
    pub stdout: Vec<u8>,
    /// stderr bytes — a diagnostic channel, never merged into content.
    pub stderr: Vec<u8>,
    /// Normal `exit 0`?
    pub exit_ok: bool,
}

impl CommandOutcome {
    /// The source observation, or None when the run did not exit 0.
    /// Empty stdout on success is a legitimate empty content.
    pub fn into_observation(self) -> Option<Observation> {
        if self.exit_ok {
            Some(Observation {
                bytes: self.stdout,
                text: true,
                encoding: Some("utf-8".into()),
            })
        } else {
            None
        }
    }
}
