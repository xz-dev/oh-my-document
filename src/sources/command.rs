//! Command sources: `command::<executable>::<JSON args array>` (E-7/9).
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

/// Parse `command::<exe>::<args>` into (executable, argv).
/// `args` must be a JSON array of *strings only* — numbers, objects, or
/// nested arrays are rejected before launch, never coerced.
/// `::` inside the args string is literal; we split only on the first two.
pub fn parse_command_ref(s: &str) -> Result<(String, Vec<String>), SourceError> {
    let rest = s.strip_prefix("command::").ok_or(SourceError::Command)?;
    let (exe, args_json) = rest.split_once("::").ok_or(SourceError::Command)?;
    if exe.is_empty() {
        return Err(SourceError::Command);
    }
    let arr: serde_json::Value =
        serde_json::from_str(args_json).map_err(|_| SourceError::Command)?;
    let items = arr.as_array().ok_or(SourceError::Command)?;
    let mut argv = Vec::new();
    for it in items {
        match it.as_str() {
            Some(v) => argv.push(v.to_string()),
            None => return Err(SourceError::Command), // non-string arg rejected
        }
    }
    Ok((exe.to_string(), argv))
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
