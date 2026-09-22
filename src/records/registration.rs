//! Immutable shared logical project registration records.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteIdentity {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRegistration {
    pub format: String,
    pub alias: String,
    pub project_id: String,
    pub store_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteIdentity>,
}

impl ProjectRegistration {
    pub fn new(
        alias: String,
        project_id: String,
        store_id: String,
        revision: u64,
        remote: Option<RemoteIdentity>,
    ) -> Self {
        Self {
            format: "omd.registration/1".into(),
            alias,
            project_id,
            store_id,
            revision,
            remote,
        }
    }

    pub fn id(&self) -> Result<String, toml::ser::Error> {
        let bytes = toml::to_string(self)?;
        Ok(hex::encode(Sha256::digest(bytes.as_bytes())))
    }
}
