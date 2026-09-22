//! Versioned structured CLI output.
//!
//! Every invocation emits one JSON object on stdout. Diagnostics keep stable
//! typed fields; command payloads remain under `data`.

use serde::Serialize;

pub const SCHEMA_VERSION: &str = "2";

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub kind: String,
    pub severity: String,
    pub message: String,
    pub store: Option<String>,
    pub node: Option<serde_json::Value>,
    pub commit_id: Option<String>,
}

impl Diagnostic {
    pub fn new(
        kind: impl Into<String>,
        severity: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            severity: severity.into(),
            message: message.into(),
            store: None,
            node: None,
            commit_id: None,
        }
    }

    pub fn context(
        mut self,
        store: Option<String>,
        node: Option<serde_json::Value>,
        commit_id: Option<String>,
    ) -> Self {
        self.store = store;
        self.node = node;
        self.commit_id = commit_id;
        self
    }
}

#[derive(Debug, Serialize)]
pub struct Envelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub data: serde_json::Value,
    pub diagnostics: Vec<Diagnostic>,
}

impl Envelope {
    pub fn new(ok: bool, data: serde_json::Value, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ok,
            data,
            diagnostics,
        }
    }
}

pub fn decimal(value: u64) -> String {
    value.to_string()
}
