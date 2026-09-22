//! Immutable recovery-binding revisions for complete source versions.

use serde::{Deserialize, Serialize};

use crate::sources::reference::SourceDescriptor;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub format: String,
    pub id: String,
    pub version_id: String,
    pub recovery: SourceDescriptor,
    pub seq: u64,
    pub affected: Vec<String>,
}

impl Binding {
    pub fn validate(&self, expected_id: &str, expected_version: &str) -> Result<(), String> {
        if self.format != "omd.binding/2" {
            return Err(format!(
                "unsupported binding format '{}' — this build reads omd.binding/2",
                self.format
            ));
        }
        if self.id != expected_id
            || self.version_id != expected_version
            || self.seq == 0
            || self.id.len() != 32
            || self.version_id.len() != 32
            || !self.id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !self.version_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.affected.is_empty()
            || self.affected.iter().any(|commit| {
                commit.len() != 64 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err("binding identity/version/sequence is inconsistent".into());
        }
        self.recovery
            .validate_portable()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceError {
    #[error("content mismatch: new source differs from recorded full content")]
    Mismatch,
    #[error("recorded commit has no complete source version")]
    NoBasis,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
