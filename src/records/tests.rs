//! Golden-vector tests for canonical commit-ID derivation (task 2.2).
//!
//! The expected hash below was computed independently of this implementation
//! (Python: sha256 over u64-BE-framed fields). It MUST NOT be regenerated
//! from this crate's serializer — that would make the test tautological.

use crate::records::commit::{Commit, CommitKind};
use crate::records::id::{
    ContentRef, OperationPayload, canonical_payload_bytes, derive_commit_id, salt_from_bytes,
};
use crate::records::ids::Id128;

fn base_commit(kind: CommitKind) -> Commit {
    Commit {
        id: None,
        salt: "abcdefghijklmnop".into(),
        previous_id: "".into(),
        timestamp: "2026-09-20T03:00:00.000000000Z".into(),
        schema: "omd.commit/1".into(),
        kind,
        content_ref: "empty".into(),
        payload: serde_json::Map::new(),
        range_tips: Default::default(),
    }
}

fn payload() -> OperationPayload {
    let mut fields = serde_json::Map::new();
    fields.insert("path".into(), serde_json::Value::String("docs/a.md".into()));
    OperationPayload {
        kind: "init".into(),
        schema: "omd.commit/1".into(),
        content_ref: ContentRef::Empty,
        fields,
    }
}

#[test]
fn golden_commit_id_vector() {
    // Python reference:
    // salt="abcdefghijklmnop", prev="", ts="2026-09-20T03:00:00.000000000Z",
    // content=b"hello",
    // payload='{"content_ref":"empty","kind":"init","path":"docs/a.md","schema":"omd.commit/1"}'
    let salt = *b"abcdefghijklmnop";
    let id = derive_commit_id(
        &salt,
        "",
        "2026-09-20T03:00:00.000000000Z",
        b"hello",
        &payload(),
    );
    assert_eq!(
        id.to_hex(),
        "7a7c80614f5d9a30bba24ac4c282a2d4949b72e0fcd76d0fe694df8e6b57ca52"
    );
}

#[test]
fn framed_fields_prevent_concatenation_collisions() {
    // Two different field splits must NOT share the same preimage.
    // e.g. previous="AB", timestamp="C" vs previous="A", timestamp="BC".
    let salt = *b"abcdefghijklmnop";
    let a = derive_commit_id(&salt, "AB", "C", b"x", &payload());
    let b = derive_commit_id(&salt, "A", "BC", b"x", &payload());
    assert_ne!(a, b);
}

#[test]
fn payload_key_order_is_canonical() {
    // Field insertion order must not change the canonical bytes.
    let mut f1 = serde_json::Map::new();
    f1.insert("a".into(), 1.into());
    f1.insert("b".into(), 2.into());
    let mut f2 = serde_json::Map::new();
    f2.insert("b".into(), 2.into());
    f2.insert("a".into(), 1.into());
    let p1 = OperationPayload {
        kind: "k".into(),
        schema: "s".into(),
        content_ref: ContentRef::Empty,
        fields: f1,
    };
    let p2 = OperationPayload {
        kind: "k".into(),
        schema: "s".into(),
        content_ref: ContentRef::Empty,
        fields: f2,
    };
    assert_eq!(canonical_payload_bytes(&p1), canonical_payload_bytes(&p2));
}

#[test]
fn salt_mapping_uses_alphanumeric_alphabet() {
    let bytes = [0u8, 61, 62, 63, 200, 255, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let s = salt_from_bytes(&bytes);
    assert!(s.iter().all(|b| b.is_ascii_alphanumeric()));
    assert_eq!(s.len(), 16);
}

#[test]
fn version_content_ref_is_not_empty() {
    let v = Id128([0x11; 16]);
    let mut fields = serde_json::Map::new();
    fields.insert("x".into(), 1.into());
    let p = OperationPayload {
        kind: "commit".into(),
        schema: "omd.commit/1".into(),
        content_ref: ContentRef::Version(v),
        fields,
    };
    let bytes = canonical_payload_bytes(&p);
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.contains(&v.to_hex()));
    assert!(!s.contains("\"empty\""));
}

// ---- task 2.1: record validation ----

#[test]
fn first_commit_rejects_nonempty_previous() {
    let mut c = base_commit(CommitKind::Init);
    c.payload.insert("path".into(), "docs/a.md".into());
    c.previous_id = "abc".into();
    assert!(c.validate(true).is_err());
}

#[test]
fn unknown_payload_field_rejected_for_kind() {
    let mut c = base_commit(CommitKind::Init);
    c.payload.insert("path".into(), "docs/a.md".into());
    c.payload
        .insert("reason".into(), "not allowed on init".into());
    assert!(c.validate(true).is_err());
}

#[test]
fn missing_required_field_rejected() {
    let c = base_commit(CommitKind::Link); // needs link_id/source/target
    assert!(c.validate(true).is_err());
}

#[test]
fn valid_init_passes() {
    let mut c = base_commit(CommitKind::Init);
    c.payload.insert("path".into(), "docs/a.md".into());
    assert!(c.validate(true).is_ok());
}

#[test]
fn file_commit_snapshot_records_range_tips() {
    // A file-level commit must capture which tip each child range pointed to,
    // so a later file reset can restore exact children — not wall-clock order.
    let mut c = base_commit(CommitKind::FileVerify);
    c.range_tips.insert("r1".into(), "tip_a".into());
    c.range_tips.insert("r2".into(), "tip_b".into());
    let payload = c.canonical_payload();
    let tips = payload
        .fields
        .get("range_tips")
        .unwrap()
        .as_object()
        .unwrap();
    assert_eq!(tips["r1"], "tip_a");
    assert_eq!(tips["r2"], "tip_b");
}

#[test]
fn timestamp_must_be_rfc3339_nanos() {
    let mut c = base_commit(CommitKind::Init);
    c.payload.insert("path".into(), "x".into());
    c.timestamp = "not a time".into();
    assert!(c.validate(true).is_err());
}

#[test]
fn salt_must_be_sixteen_alnum() {
    let mut c = base_commit(CommitKind::Init);
    c.payload.insert("path".into(), "x".into());
    c.salt = "short".into();
    assert!(c.validate(true).is_err());
    c.salt = "this-is-16-char!".into(); // '!' and '-' not in alphabet
    assert!(c.validate(true).is_err());
}

// ---- task 2.3: source versions & identities ----

use crate::records::version::{Acquisition, SourceVersion};

#[test]
fn version_created_before_commit_no_self_reference() {
    // The version id is drawn before the commit is derived, so the commit's
    // content_ref can cite a stable id that does not depend on the commit id.
    let ver_id = Id128::draw(&crate::testing::FixedRng::new(42));
    let v = SourceVersion::new(
        ver_id,
        b"content",
        Acquisition::File {
            path: "docs/a.md".into(),
            encoding: "utf-8".into(),
        },
        Some("utf-8".into()),
    );
    let mut c = base_commit(CommitKind::Commit);
    c.content_ref = v.id.to_hex();
    // content_ref is the version's hex, not the commit's own id — no cycle.
    assert_ne!(c.content_ref, "");
    assert!(c.validate(true).is_ok());
}

#[test]
fn equal_observations_reuse_version_identity() {
    let id = Id128([7; 16]);
    let a = SourceVersion::new(
        id,
        b"same bytes",
        Acquisition::File {
            path: "f".into(),
            encoding: "utf-8".into(),
        },
        Some("utf-8".into()),
    );
    let b = SourceVersion::new(
        id,
        b"same bytes",
        Acquisition::File {
            path: "f".into(),
            encoding: "utf-8".into(),
        },
        Some("utf-8".into()),
    );
    assert_eq!(a.reuse_key(), b.reuse_key());
}

#[test]
fn different_content_gives_different_hash() {
    let id = Id128([7; 16]);
    let a = SourceVersion::new(
        id,
        b"x",
        Acquisition::File {
            path: "f".into(),
            encoding: "utf-8".into(),
        },
        None,
    );
    let b = SourceVersion::new(
        id,
        b"y",
        Acquisition::File {
            path: "f".into(),
            encoding: "utf-8".into(),
        },
        None,
    );
    assert_ne!(a.sha256, b.sha256);
}

#[test]
fn id128_hex_roundtrip() {
    let id = Id128::draw(&crate::testing::FixedRng::new(9));
    let parsed = Id128::from_hex(&id.to_hex()).unwrap();
    assert_eq!(id, parsed);
    assert_eq!(id.to_hex().len(), 32);
}
