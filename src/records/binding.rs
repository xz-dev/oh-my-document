//! Source replacement: rebind a recorded version to a new source that
//! provides the *same complete content* (E-10.2/10.3).
//!
//! `replace <commit-id> --source <ref>` selects the full source version the
//! commit references. The new source must deliver byte-identical *complete*
//! content — an equal range fragment is never enough. On match we write an
//! immutable binding revision (separate from the commit's original inputs)
//! that changes how that version's content is fetched, never the content
//! itself, never the commit id/inputs/links/notes. All records sharing the
//! version id rebind together; a *different* version with equal content hash
//! never rebatches.

use serde::{Deserialize, Serialize};

/// A binding revision: version_id → new acquisition. Immutable, appended —
/// the original binding is never edited in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    /// Independent binding record id.
    pub id: String,
    /// The source-version id being rebound.
    pub version_id: String,
    /// Serialized new acquisition descriptor (source ref).
    pub acquisition: serde_json::Value,
    /// Publication seq — revision order, not wall-clock.
    pub seq: u64,
    /// Records sharing this version that were rebound (impact report).
    pub affected: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceError {
    #[error("content mismatch: new source differs from recorded full content")]
    Mismatch,
    #[error("recorded version has no recoverable full content")]
    NoBasis,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Write a binding revision into `bindings/<id>.toml`.
pub fn write_binding(root: &std::path::Path, b: &Binding) -> Result<String, std::io::Error> {
    let txt = toml::to_string(b).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(root.join(format!("bindings/{}.toml", b.id)), txt)?;
    Ok(b.id.clone())
}
