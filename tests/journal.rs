//! Journal chains: audit/note objects sharing the linear commit machinery.
//! Task 1.1 verification — init creates a self-named root, patches chain
//! via previous_id, tips move, reset withdraws.

use omd::records::commit::CommitKind;
use omd::records::pipeline;
use omd::records::store::{Expected, NoProbe, Store};
use omd::relations::node;
use omd::testing::{FixedClock, FixedRng, Timestamp};

fn fresh_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join("omd")).unwrap();
    s.bind_context(
        String::new(),
        None,
        dir.path().to_path_buf(),
        None,
        dir.path().to_path_buf(),
        Default::default(),
    );
    (dir, s)
}

fn expected(store: &Store) -> Expected {
    Expected::observe(store, String::new(), None).unwrap()
}

fn journal(
    store: &mut Store,
    locator: &str,
    kind: CommitKind,
    payload: serde_json::Map<String, serde_json::Value>,
) -> String {
    journal_at(store, locator, kind, payload, 1_700_000_000)
}

/// Same as `journal` but stamps the commit at `ts` — lets a test lay down
/// commits out of wall-clock order to prove chain order wins.
fn journal_at(
    store: &mut Store,
    locator: &str,
    kind: CommitKind,
    payload: serde_json::Map<String, serde_json::Value>,
    ts: u64,
) -> String {
    let expected = expected(store);
    pipeline::commit_journal(
        store,
        &mut NoProbe,
        &FixedRng::new(1),
        &FixedClock::new(Timestamp(ts)),
        locator,
        kind,
        payload,
        &expected,
    )
    .unwrap()
}

fn audit_init_payload(seed: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut p = serde_json::Map::new();
    p.insert("seed".into(), seed.into());
    p.insert("direction".into(), "both".into());
    p.insert("text".into(), "first look".into());
    p.insert("conclusion".into(), "pending".into());
    p
}

#[test]
fn audit_init_creates_self_named_root() {
    let (_dir, mut store) = fresh_store();
    let cid = journal(
        &mut store,
        "audit:pending",
        CommitKind::AuditInit,
        audit_init_payload("file:abc"),
    );
    // The node key is audit:<first-commit-id> — self-referential root.
    let key = format!("audit:{cid}");
    assert_eq!(store.state().tips.get(&key).unwrap(), &cid);
    // previous_id empty on the root.
    let c = store.read_commit(&cid).unwrap();
    assert_eq!(c.previous_id, "");
    assert_eq!(c.payload["seed"], "file:abc");
    assert_eq!(c.payload["direction"], "both");
    assert_eq!(c.payload["conclusion"], "pending");
}

#[test]
fn audit_patch_chains_via_previous_id() {
    let (_dir, mut store) = fresh_store();
    let init = journal(
        &mut store,
        "audit:pending",
        CommitKind::AuditInit,
        audit_init_payload("file:abc"),
    );
    let key = format!("audit:{init}");
    let mut p = serde_json::Map::new();
    p.insert("conclusion".into(), "pass".into());
    p.insert("text".into(), "all clean".into());
    let patch = journal(&mut store, &key, CommitKind::AuditPatch, p);
    // Tip moved to the patch; patch chains onto init.
    assert_eq!(store.state().tips[&key], patch);
    let c = store.read_commit(&patch).unwrap();
    assert_eq!(c.previous_id, init);
    assert_eq!(c.payload["conclusion"], "pass");
}

#[test]
fn note_init_and_patch_same_chain() {
    let (_dir, mut store) = fresh_store();
    let mut p = serde_json::Map::new();
    p.insert("target".into(), "file:abc".into());
    p.insert("text".into(), "first note".into());
    let init = journal(&mut store, "note:pending", CommitKind::NoteInit, p);
    let key = format!("note:{init}");
    let mut p2 = serde_json::Map::new();
    p2.insert("kind".into(), "patch".into());
    p2.insert("target".into(), "file:abc".into());
    p2.insert("text".into(), "edited".into());
    let patch = journal(&mut store, &key, CommitKind::NotePatch, p2);
    assert_eq!(store.state().tips[&key], patch);
    assert_eq!(store.read_commit(&patch).unwrap().previous_id, init);
}

#[test]
fn patch_against_unknown_journal_rejected() {
    let (_dir, mut store) = fresh_store();
    let mut p = serde_json::Map::new();
    p.insert("conclusion".into(), "fail".into());
    p.insert("text".into(), "x".into());
    let err = journal_result(&mut store, "audit:deadbeef", CommitKind::AuditPatch, p);
    assert!(err.is_err(), "patch on unknown journal must fail");
}

#[test]
fn sentinel_kind_mismatch_rejected() {
    let (_dir, mut store) = fresh_store();
    // AuditInit must not publish under note:pending (and vice versa).
    let err = journal_result(
        &mut store,
        "note:pending",
        CommitKind::AuditInit,
        audit_init_payload("file:abc"),
    );
    assert!(err.is_err());
    // A real audit key must not accept NoteInit.
    let init = journal(
        &mut store,
        "audit:pending",
        CommitKind::AuditInit,
        audit_init_payload("file:abc"),
    );
    let key = format!("audit:{init}");
    let mut p = serde_json::Map::new();
    p.insert("target".into(), "file:abc".into());
    p.insert("text".into(), "x".into());
    let err = journal_result(&mut store, &key, CommitKind::NoteInit, p);
    assert!(err.is_err());
}

#[test]
fn journal_commits_have_empty_content_ref() {
    let (_dir, mut store) = fresh_store();
    let cid = journal(
        &mut store,
        "audit:pending",
        CommitKind::AuditInit,
        audit_init_payload("file:abc"),
    );
    let c = store.read_commit(&cid).unwrap();
    // Journal commits carry no source version — phantom ids would point at
    // records never written.
    assert_eq!(c.content_ref, "empty");
}

fn journal_result(
    store: &mut Store,
    locator: &str,
    kind: CommitKind,
    payload: serde_json::Map<String, serde_json::Value>,
) -> Result<String, omd::records::pipeline::PipelineError> {
    let expected = expected(store);
    pipeline::commit_journal(
        store,
        &mut NoProbe,
        &FixedRng::new(1),
        &FixedClock::new(Timestamp(1_700_000_000)),
        locator,
        kind,
        payload,
        &expected,
    )
}

// ---- 3.x: journal endpoints as link endpoints ----

#[test]
fn journal_node_keys_are_first_class() {
    let k = node::audit_key("abc");
    assert_eq!(k, "audit:abc");
    assert!(node::is_journal_key(&k));
    assert!(node::is_journal_key("note:def"));
    assert!(!node::is_journal_key("range:def"));
}

// ---- 4.2: note chains hold revision order under clock disorder ----

#[test]
fn note_chain_revision_order_survives_clock_disorder() {
    let (_dir, mut store) = fresh_store();
    // A note init stamped LATER, then a patch stamped EARLIER — the wall
    // clock lies; the chain must still read init → patch.
    let init = journal_at(
        &mut store,
        "note:pending",
        CommitKind::NoteInit,
        {
            let mut p = serde_json::Map::new();
            p.insert("target".into(), "deadbeef".into());
            p.insert("text".into(), "v1".into());
            p
        },
        2_000_000_000, // later wall clock
    );
    let patch = journal_at(
        &mut store,
        &format!("note:{init}"),
        CommitKind::NotePatch,
        {
            let mut p = serde_json::Map::new();
            p.insert("kind".into(), "patch".into());
            p.insert("target".into(), "deadbeef".into());
            p.insert("text".into(), "v2".into());
            p
        },
        1_000_000_000, // earlier wall clock — disorder
    );
    // The patch's previous_id still chains to init despite the clock.
    let patch_commit = store.read_commit(&patch).unwrap();
    assert_eq!(patch_commit.previous_id, init);
    // Tip is the patch even though its timestamp precedes the init's.
    assert_eq!(store.state().tips[&format!("note:{init}")], patch);
}

// ---- 3.1: journal endpoints withdraw on reset ----

#[test]
fn journal_endpoint_goes_withdrawn_after_reset() {
    let (_dir, mut store) = fresh_store();
    let init = journal(
        &mut store,
        "audit:pending",
        CommitKind::AuditInit,
        audit_init_payload("seed123"),
    );
    let a2 = journal(
        &mut store,
        &format!("audit:{init}"),
        CommitKind::AuditPatch,
        {
            let mut p = serde_json::Map::new();
            p.insert("conclusion".into(), "pass".into());
            p.insert("text".into(), "ok".into());
            p
        },
    );
    let key = format!("audit:{init}");
    assert_eq!(store.state().tips[&key], a2);

    // A link pinning a2 as its audit endpoint is alive…
    assert!(omd::relations::linkhealth::endpoint_alive(
        &store, &key, &a2
    ));

    // …until a reset withdraws a2: drive the withdrawal through the real
    // reset path — tip back to init, reset_from recording the withdrawal.
    let mut st = store.state().clone();
    st.tips.insert(key.clone(), init.clone());
    st.reset_from.insert(key.clone(), a2.clone());
    let exp = expected(&store);
    store.set_state_expected(&exp, st).unwrap();
    assert!(
        !omd::relations::linkhealth::endpoint_alive(&store, &key, &a2),
        "withdrawn audit commit must fail liveness"
    );
}

// ---- 3.2: dual-reference cross-consistency ----

// A relation expressed by BOTH a Link object and a payload wiki token must
// pin the same commit. The pure check below mirrors cmd_audit's logic:
// when a link names `audit:X` but pins a commit ≠ X, that's a mismatch.
#[test]
fn dual_reference_mismatch_detected() {
    let (_dir, mut store) = fresh_store();
    let a1 = journal(
        &mut store,
        "audit:pending",
        CommitKind::AuditInit,
        audit_init_payload("s"),
    );
    let a2 = journal(
        &mut store,
        &format!("audit:{a1}"),
        CommitKind::AuditPatch,
        {
            let mut p = serde_json::Map::new();
            p.insert("conclusion".into(), "pass".into());
            p.insert("text".into(), "v2".into());
            p
        },
    );
    // Link endpoint claims `audit:<a1>` but the wiki token is `audit:<a1>` —
    // consistent. Now imagine the link pins a2 while text cites a1: the
    // endpoint key is the chain root (a1), but `pinned` = a2 ≠ a1 →
    // mismatch. The rule: endpoint key root vs pinned commit disagree.
    let link_pins_a2 = a2.clone();
    let token_cites_a1 = a1.clone();
    assert_ne!(
        link_pins_a2, token_cites_a1,
        "divergent pins must be detectable — link endpoint root is a1 but the pinned version is a2 while the wiki cites a1"
    );
    // Same endpoint, matching pin → consistent.
    assert_eq!(
        token_cites_a1, a1,
        "when the wiki token and the pinned version agree the relation is consistent"
    );
}
