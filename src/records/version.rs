//! Immutable complete source versions.
//!
//! Current observation definition and historical recovery definition are
//! separate. Mutable replacement revisions may change only recovery; they
//! never switch how current content is observed.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::records::ids::Id128;
pub use crate::sources::reference::SourceDescriptor as Acquisition;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVersion {
    pub format: String,
    pub id: Id128,
    pub len: u64,
    pub sha256: String,
    /// Definition used for current observation.
    pub acquisition: Acquisition,
    /// Immutable initial recovery definition. A selected Binding revision may
    /// supersede this without changing acquisition or immutable history.
    pub recovery: Acquisition,
    pub encoding: Option<String>,
    pub content_file: Option<String>,
}

impl SourceVersion {
    pub fn content_sha256(content: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(content);
        hex::encode(h.finalize())
    }

    pub fn new(
        id: Id128,
        content: &[u8],
        acquisition: Acquisition,
        encoding: Option<String>,
    ) -> Self {
        Self::new_with_recovery(id, content, acquisition.clone(), acquisition, encoding)
    }

    pub fn new_with_recovery(
        id: Id128,
        content: &[u8],
        acquisition: Acquisition,
        recovery: Acquisition,
        encoding: Option<String>,
    ) -> Self {
        Self {
            format: "omd.version/2".into(),
            id,
            len: content.len() as u64,
            sha256: Self::content_sha256(content),
            acquisition,
            recovery,
            encoding,
            content_file: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != "omd.version/2" {
            return Err(format!(
                "unsupported source version format '{}' — this build reads omd.version/2",
                self.format
            ));
        }
        self.acquisition
            .validate_portable()
            .map_err(|error| error.to_string())?;
        self.recovery
            .validate_portable()
            .map_err(|error| error.to_string())?;
        if self.sha256.len() != 64
            || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self
                .content_file
                .as_deref()
                .is_some_and(|path| path != format!("content/{}", self.sha256))
        {
            return Err("source version hash/content path is invalid".into());
        }
        if self.len == 0 && self.sha256 != Self::content_sha256(&[]) {
            return Err("source version length/hash mismatch".into());
        }
        Ok(())
    }

    pub fn reuse_key(&self) -> (String, String) {
        (format!("{:?}", self.acquisition), self.sha256.clone())
    }
}
