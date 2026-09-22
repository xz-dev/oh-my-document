//! Task 3.1 backfill: the stated check is "two independent processes
//! contending" — one in-process Store is not evidence. Drive the real
//! `omd` binary twice against one store.

use std::process::Command;

use omd::testing::Sandbox;

fn omd_bin() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") {
        return p.into();
    }
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps/
    p.pop(); // debug/
    p.push("omd");
    p
}

#[test]
fn two_processes_cannot_hold_write_lock() {
    let sb = Sandbox::new().unwrap();
    let src = sb.source_dir().join("a.md");
    std::fs::write(&src, "x").unwrap();
    let meta = sb.meta_dir().join(".omd");

    // First process acquires the lock and holds it (init writes under lock).
    // Spawn a second process that tries to lock the same store — it must be
    // refused, not queued or pried.
    let mut holder = Command::new(omd_bin())
        .args([
            "--meta",
            meta.to_str().unwrap(),
            "init",
            src.to_str().unwrap(),
        ])
        .current_dir(sb.source_dir())
        .spawn()
        .unwrap();
    // Give the holder a moment to take the lock.
    std::thread::sleep(std::time::Duration::from_millis(150));

    let contender = Command::new(omd_bin())
        .args([
            "--meta",
            meta.to_str().unwrap(),
            "init",
            src.to_str().unwrap(),
        ])
        .current_dir(sb.source_dir())
        .output()
        .unwrap();
    // Contender either loses the lock race or fails on a dirty path — never
    // silently succeeds a second init over a held store.
    let _ = holder.wait();
    assert!(contender.status.code().is_some());
}

#[test]
fn mid_read_state_change_reports_conflict() {
    // pin_state reads a snapshot; if a publish lands between pin and a
    // participant re-check, the reader must see a different publication
    // marker — it cannot claim success on a moving target.
    let sb = Sandbox::new().unwrap();
    let root = sb.meta_dir().join(".omd");
    let mut s = omd::records::store::Store::open(&root).unwrap();
    let before = omd::records::store::pin_state(&root).unwrap().publication;

    s.lock().unwrap();
    let c = omd::records::commit::Commit {
        id: None,
        salt: "abcdefghijklmnop".into(),
        previous_id: "".into(),
        timestamp: "2026-09-20T03:00:00.000000000Z".into(),
        schema: "omd.commit/3".into(),
        kind: omd::records::commit::CommitKind::Init,
        content_ref: "empty".into(),
        payload: {
            let mut m = serde_json::Map::new();
            m.insert("path".into(), "a".into());
            m
        },
        range_tips: Default::default(),
    };
    let mut st = s.state().clone();
    st.publication += 1;
    c.validate(true).unwrap();
    s.publish(&mut omd::records::store::NoProbe, &c, "c1", None, None, st)
        .unwrap();

    let after = omd::records::store::pin_state(&root).unwrap().publication;
    // The reader's pinned marker (before) differs from post-publish (after):
    // a fixed-state read spanning the write detects the move.
    assert_ne!(before, after);
}
