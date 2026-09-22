//! Task 3.x: single-writer lock, expected-version preconditions, staged
//! atomic publication, and crash-recovery semantics.

use std::path::Path;
use std::rc::Rc;

use omd::records::commit::{Commit, CommitKind};
use omd::records::store::{Expected, NoProbe, Store, pin_state};
use omd::records::version::{Acquisition, SourceVersion};
use omd::testing::PublishStage;
use omd::testing::{FaultInjector, Sandbox};

fn commit(kind: CommitKind, salt: &str) -> Commit {
    let mut c = Commit {
        id: None,
        salt: salt.into(),
        previous_id: String::new(),
        timestamp: "2026-09-20T03:00:00.000000000Z".into(),
        schema: "omd.commit/3".into(),
        kind,
        content_ref: "empty".into(),
        payload: serde_json::Map::new(),
        range_tips: Default::default(),
    };
    if kind == CommitKind::Init {
        c.payload.insert("path".into(), "docs/a.md".into());
    }
    c
}

fn open_store(root: &Path) -> Store {
    Store::open(root).expect("open store")
}

#[test]
fn lock_conflicts_are_rejected_not_waited() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut a = open_store(&sb.meta_dir());
    a.lock().unwrap();
    // A second open on the same dir must fail fast, not block or retry.
    let mut b = open_store(&sb.meta_dir());
    assert!(matches!(
        b.lock(),
        Err(omd::records::store::StoreError::Lock)
    ));
}

#[test]
fn expected_version_conflict_aborts() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut s = open_store(&sb.meta_dir());
    s.lock().unwrap();
    let exp = Expected {
        publication: 999,
        ..Default::default()
    };
    assert!(s.check_expected(&exp).is_err());
}

#[test]
fn staged_failure_before_rename_keeps_old_state() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut s = open_store(&sb.meta_dir());
    s.lock().unwrap();
    let old_pub = s.state().publication;

    let mut inj = FaultInjector::new();
    inj.arm_once(PublishStage::WriteTempState, 1);
    let c = commit(CommitKind::Init, "abcdefghijklmnop");
    let mut new_state = s.state().clone();
    new_state.publication += 1;
    new_state.tips.insert("file:a".into(), "deadbeef".into());
    assert!(
        s.publish(&mut inj, &c, "c1", None, None, new_state)
            .is_err()
    );

    // Old state must still be readable and unchanged.
    let pinned = pin_state(&sb.meta_dir()).unwrap();
    assert_eq!(pinned.publication, old_pub);
    assert!(!pinned.tips.contains_key("file:a"));
}

#[test]
fn post_rename_publishes_full_new_state() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut s = open_store(&sb.meta_dir());
    s.lock().unwrap();
    let c = commit(CommitKind::Init, "abcdefghijklmnop");
    let mut new_state = s.state().clone();
    new_state.publication += 1;
    new_state.tips.insert("file:a".into(), "c1".into());
    s.publish(&mut NoProbe, &c, "c1", None, None, new_state)
        .unwrap();
    let pinned = pin_state(&sb.meta_dir()).unwrap();
    assert_eq!(pinned.tips["file:a"], "c1");
}

#[test]
fn immutable_records_use_per_id_paths() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut s = open_store(&sb.meta_dir());
    s.lock().unwrap();
    let c = commit(CommitKind::Init, "abcdefghijklmnop");
    let mut st = s.state().clone();
    st.publication += 1;
    s.publish(&mut NoProbe, &c, "commit-AAA", None, None, st)
        .unwrap();
    // Second commit must land in its own file, not overwrite the first.
    let c2 = commit(CommitKind::Commit, "bcdefghijklmnopq");
    let mut st2 = s.state().clone();
    st2.publication += 1;
    s.publish(&mut NoProbe, &c2, "commit-BBB", None, None, st2)
        .unwrap();
    assert!(sb.meta_dir().join("commits/commit-AAA.toml").exists());
    assert!(sb.meta_dir().join("commits/commit-BBB.toml").exists());
}

#[test]
fn version_and_content_written_per_id_and_hash() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut s = open_store(&sb.meta_dir());
    s.lock().unwrap();
    let v = SourceVersion::new(
        omd::records::ids::Id128([1; 16]),
        b"body",
        Acquisition::File {
            project: "root".into(),
            path: "f".into(),
        },
        Some("utf-8".into()),
    );
    let c = commit(CommitKind::Init, "abcdefghijklmnop");
    let mut st = s.state().clone();
    st.publication += 1;
    s.publish(&mut NoProbe, &c, "c1", Some(&v), Some(b"body"), st)
        .unwrap();
    assert!(
        sb.meta_dir()
            .join(format!("versions/{}.toml", v.id.to_hex()))
            .exists()
    );
    assert!(sb.meta_dir().join(format!("content/{}", v.sha256)).exists());
}

#[test]
fn lost_response_detected_by_operation_id() {
    // After a successful publish, the persisted operation_id lets a caller
    // that never received a response recognize its own update on retry.
    let sb = Rc::new(Sandbox::new().unwrap());
    let mut s = open_store(&sb.meta_dir());
    s.lock().unwrap();
    let c = commit(CommitKind::Init, "abcdefghijklmnop");
    let mut st = s.state().clone();
    st.publication += 1;
    st.operation_id = Some("op-123".into());
    s.publish(&mut NoProbe, &c, "c1", None, None, st).unwrap();
    let pinned = pin_state(&sb.meta_dir()).unwrap();
    assert_eq!(pinned.operation_id.as_deref(), Some("op-123"));
}
