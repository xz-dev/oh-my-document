//! Canonical commit-ID derivation (design E-2).
//!
//! `id = SHA256( salt_ascii_16
//!             || frame(previous_id_utf8)
//!             || frame(timestamp_utf8)
//!             || frame(full_content_bytes)
//!             || frame(JCS(operation_payload)) )`
//!
//! `frame(x) = u64 big-endian length prefix + x`. Length framing makes field
//! boundaries unambiguous — two different field splits can never collide on
//! the same input. Salt is always the first 16 bytes, drawn from the OS
//! CSPRNG alphabet `[A-Za-z0-9]` in production.

use sha2::{Digest, Sha256};

use crate::records::ids::Id128;

/// A SHA-256 commit/content identifier, hex-displayed (64 chars).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CommitId(pub [u8; 32]);

impl CommitId {
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Accept a full 64-hex ID; unique-prefix resolution happens in lookups.
    pub fn from_hex(s: &str) -> Result<Self, IdError> {
        let b = hex::decode(s).map_err(|_| IdError::BadHex)?;
        if b.len() != 32 {
            return Err(IdError::BadLength);
        }
        let mut a = [0u8; 32];
        a.copy_from_slice(&b);
        Ok(Self(a))
    }
}

impl std::fmt::Display for CommitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("bad hex in id")]
    BadHex,
    #[error("id must be 32 bytes")]
    BadLength,
}

fn frame(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u64).to_be_bytes());
    buf.extend_from_slice(data);
}

/// Canonical payload: schema/kind/content_ref plus closed operation fields.
/// Serialized to bytes by `canonical_payload_bytes` before hashing.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationPayload {
    /// Object kind, e.g. "init", "commit", "atomic_begin".
    pub kind: String,
    /// Versioned schema tag for the payload shape.
    pub schema: String,
    /// Reference to the source-version record the content bytes came from.
    pub content_ref: ContentRef,
    /// Closed per-kind operation fields (paths, coordinates, reasons, etc.).
    pub fields: serde_json::Map<String, serde_json::Value>,
}

/// What content the commit used: a source-version id, or the literal `empty`.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentRef {
    Empty,
    Version(Id128),
}

impl ContentRef {
    fn as_json(&self) -> serde_json::Value {
        match self {
            ContentRef::Empty => serde_json::Value::String("empty".into()),
            ContentRef::Version(id) => serde_json::Value::String(id.to_hex()),
        }
    }
}

/// Serialize payload to its canonical byte form.
/// JSON objects are emitted with sorted keys; all strings are raw UTF-8
/// (no normalization). This is a deterministic subset of JCS (RFC 8785):
/// sorted keys, compact separators, strings escaped per JSON rules.
pub fn canonical_payload_bytes(p: &OperationPayload) -> Vec<u8> {
    let mut obj = p.fields.clone();
    obj.insert("content_ref".into(), p.content_ref.as_json());
    obj.insert("kind".into(), serde_json::Value::String(p.kind.clone()));
    obj.insert("schema".into(), serde_json::Value::String(p.schema.clone()));
    let v = serde_json::Value::Object(obj);
    jcs(&v, &mut Vec::new())
}

/// Minimal deterministic JSON writer: sorted object keys, compact, and no
/// whitespace. Numbers are emitted exactly as stored (the caller must put
/// large integers in strings to avoid float loss — enforced upstream).
fn jcs(v: &serde_json::Value, out: &mut Vec<u8>) -> Vec<u8> {
    match v {
        serde_json::Value::Null => out.extend_from_slice(b"null"),
        serde_json::Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        serde_json::Value::Number(n) => out.extend_from_slice(n.to_string().as_bytes()),
        serde_json::Value::String(s) => {
            // serde_json::to_string produces RFC-compliant escaping.
            out.extend_from_slice(serde_json::to_string(s).unwrap().as_bytes())
        }
        serde_json::Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                jcs(item, out);
            }
            out.push(b']');
        }
        serde_json::Value::Object(map) => {
            out.push(b'{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(serde_json::to_string(k).unwrap().as_bytes());
                out.push(b':');
                jcs(&map[*k], out);
            }
            out.push(b'}');
        }
    }
    out.to_vec()
}

/// Derive a commit ID from its immutable inputs.
///
/// `previous` is `""` for a node's first commit. `timestamp` is the canonical
/// UTC RFC3339-nanosecond string. `content` is the full raw source bytes the
/// operation consumed (empty for pure-structural markers). `payload` is the
/// closed operation description; schema/kind/content_ref are injected.
pub fn derive_commit_id(
    salt: &[u8; 16],
    previous: &str,
    timestamp: &str,
    content: &[u8],
    payload: &OperationPayload,
) -> CommitId {
    let mut buf = Vec::with_capacity(16 + content.len() + 512);
    buf.extend_from_slice(salt);
    frame(&mut buf, previous.as_bytes());
    frame(&mut buf, timestamp.as_bytes());
    frame(&mut buf, content);
    frame(&mut buf, &canonical_payload_bytes(payload));
    let mut h = Sha256::new();
    h.update(&buf);
    let digest = h.finalize();
    let mut a = [0u8; 32];
    a.copy_from_slice(&digest);
    CommitId(a)
}

/// Draw a 16-char `[A-Za-z0-9]` salt from a byte source (OS CSPRNG in prod).
pub fn salt_from_bytes(bytes: &[u8; 16]) -> [u8; 16] {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut s = [0u8; 16];
    for (i, b) in bytes.iter().enumerate() {
        // 62 does not divide 256 evenly; rejection would need a stream.
        // We take bytes two-at-a-time scaled into 62 — for test determinism
        // we keep it simple and map via modulo; production uses the same
        // function fed by OS bytes, bias is acceptable for salt purposes.
        s[i] = ALPHABET[(b % 62) as usize];
    }
    s
}
