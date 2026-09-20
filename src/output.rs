//! The unified `--json` envelope every command emits on stdout (E-12.4).
//!
//! One parseable JSON object per invocation — stderr stays clean for
//! diagnostics. Unknown current content is `incomplete`/`null`, never a fake
//! empty-content 100%. Reset results carry requested vs actual plus warning;
//! partial successes and indeterminate publishes are machine-distinguishable.

use serde::Serialize;

/// The single stdout envelope. `ok` is the top-level success bit; `code`
/// mirrors the process exit code so scripts needn't read the status.
#[derive(Debug, Serialize)]
pub struct Envelope {
    pub ok: bool,
    /// Numeric exit code (0 success, 1 generic failure, 2 usage).
    pub code: i32,
    /// The command-specific payload (report, commit id, coverage, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Diagnostics that must not corrupt the payload channel.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Partial-success detail: which sub-operations succeeded before a
    /// failure (machine-distinguishable, never a bare error string).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partial: Vec<String>,
}

impl Envelope {
    pub fn ok(result: serde_json::Value) -> Self {
        Self { ok: true, code: 0, result: Some(result), diagnostics: vec![], partial: vec![] }
    }
    pub fn err(msg: impl Into<String>, code: i32) -> Self {
        Self { ok: false, code, result: None, diagnostics: vec![msg.into()], partial: vec![] }
    }
    /// Unknown current content — `incomplete`, not a fabricated 100%.
    pub fn incomplete(what: impl Into<String>) -> serde_json::Value {
        serde_json::json!({ "status": "incomplete", "subject": what.into(), "coverage": null })
    }
    /// A reset result: requested vs actual landing + warning.
    pub fn reset(requested: &str, actual: &str, warning: &str) -> serde_json::Value {
        serde_json::json!({
            "reset": { "requested": requested, "actual": actual },
            "warning": warning,
        })
    }
    /// A composite that partially succeeded — lists completed sub-ops.
    pub fn partial(completed: Vec<String>, err: impl Into<String>) -> Self {
        Self { ok: false, code: 1, result: None, diagnostics: vec![err.into()], partial: completed }
    }
}
