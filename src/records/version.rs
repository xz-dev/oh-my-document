//! Source versions: full-content snapshots that commits reference (E-2).
//!
//! A source version is created *before* the commit that cites it, so the
//! commit's `content_ref` can point at a stable 128-bit id without the
//! version depending on the commit's own ID. Versions carry the complete
//! byte length + SHA-256 + decode info and acquisition description; equal
//! (source-definition, decode-view, content) observations may reuse the same
//! version instead of minting a new one, and distinct versions can share one
//! `content/<sha256>` file.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::records::ids::Id128;

/// How the bytes were obtained. The decoding view is part of reuse identity —
/// the same bytes under a different declared encoding are a different version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Acquisition {
    /// Read a real file from its registered path.
    File { path: String, encoding: String },
    /// Captured stdout of an authorized command.
    Command {
        executable: String,
        args: Vec<String>,
    },
    /// Reused from a precise Git object (commit + path-in-commit).
    Git {
        repo: String,
        commit: String,
        path: String,
    },
}

/// A source version record (persisted under `versions/<id>.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceVersion {
    /// Opaque 128-bit id, drawn before the citing commit is written.
    pub id: Id128,
    /// Byte length of the full content.
    pub len: u64,
    /// SHA-256 of the full raw bytes (hex).
    pub sha256: String,
    /// How the content was acquired / where it can be recovered.
    pub acquisition: Acquisition,
    /// For text sources: the encoding used to interpret coordinates.
    pub encoding: Option<String>,
    /// Where the raw bytes live (`content/<sha256>`) when stored in OMD;
    /// `None` for git-reused content that is not separately copied.
    pub content_file: Option<String>,
}

impl SourceVersion {
    /// Hash the full content bytes (canonical identity for sharing).
    pub fn content_sha256(content: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(content);
        hex::encode(h.finalize())
    }

    /// Create a version for a fresh observation. `id` must come from the
    /// caller's RNG *before* any commit is derived — this avoids a
    /// self-referential ID.
    pub fn new(
        id: Id128,
        content: &[u8],
        acquisition: Acquisition,
        encoding: Option<String>,
    ) -> Self {
        Self {
            id,
            len: content.len() as u64,
            sha256: Self::content_sha256(content),
            acquisition,
            encoding,
            content_file: None,
        }
    }

    /// Reuse key: two observations are the same version when the acquisition
    /// definition, decode view, and content hash all match. Timestamps and
    /// "last checked" never enter this.
    pub fn reuse_key(&self) -> (String, String) {
        (format!("{:?}", self.acquisition), self.sha256.clone())
    }
}
