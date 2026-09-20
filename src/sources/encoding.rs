//! Encoding resolution: user-selectable decode order (design E-5).
//!
//! Priority, highest first:
//!   1. `--encoding` CLI flag
//!   2. encoding recorded on the source's last observation
//!   3. per-file config
//!   4. project default
//!   5. user-global default
//!   6. UTF-8
//!
//! The *first* source in the chain that supplies a value wins; later values
//! never reinterpret history — a recorded encoding freezes that observation.

/// Inputs to resolution. `None` means "this layer didn't specify".
#[derive(Debug, Default)]
pub struct EncodingChoice {
    pub cli: Option<String>,
    pub recorded: Option<String>,
    pub file_config: Option<String>,
    pub project_default: Option<String>,
    pub user_default: Option<String>,
}

/// Resolve the effective encoding. Always returns a concrete name —
/// UTF-8 is the floor when nothing else chose.
pub fn resolve(c: &EncodingChoice) -> String {
    c.cli
        .as_ref()
        .or(c.recorded.as_ref())
        .or(c.file_config.as_ref())
        .or(c.project_default.as_ref())
        .or(c.user_default.as_ref())
        .cloned()
        .unwrap_or_else(|| "utf-8".to_string())
}
