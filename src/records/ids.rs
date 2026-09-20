//! 128-bit identities for stores, source versions, links, and notes
//! (design E-2/E-4/E-5). Distinct from `CommitId`: these are opaque
//! random identifiers, never derived from content.

use serde::{Deserialize, Serialize};

/// A 128-bit opaque identifier (store_id, source-version id, link id,
/// note id). Displayed as 32 lowercase hex chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id128(pub [u8; 16]);

impl Id128 {
    /// Build from 16 raw bytes (OS CSPRNG in production, FixedRng in tests).
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Draw from an RNG byte source.
    pub fn draw(rng: &dyn crate::testing::Rng) -> Self {
        let mut b = [0u8; 16];
        rng.fill(&mut b);
        Self(b)
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, super::id::IdError> {
        let b = hex::decode(s).map_err(|_| super::id::IdError::BadHex)?;
        if b.len() != 16 {
            return Err(super::id::IdError::BadLength);
        }
        let mut a = [0u8; 16];
        a.copy_from_slice(&b);
        Ok(Self(a))
    }
}

impl std::fmt::Display for Id128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}
