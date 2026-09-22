//! 14.x structured-tracking-references: node identity, ref resolution,
//! and position fields — the new model this change adds.

mod common;
use std::process::Command;

use omd::records::commit::CommitKind;
use omd::records::pipeline;
use omd::records::store::Expected;
use omd::records::store::{NoProbe, Store};
use omd::records::version::SourceVersion;
use omd::relations::identity::{self, Kind, Position, Ref, RefSpec, Span};
use omd::testing::{FixedClock, FixedRng, Timestamp};

fn omd_bin() -> std::path::PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_omd") {
        return path.into();
    }
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("omd");
    path
}

fn run_cli(root: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(omd_bin())
        .arg("--meta")
        .arg(root.join(".omd"))
        .args(common::with_expected(
            &omd_bin(),
            root,
            args,
            Some(&root.join(".omd")),
            Some(&root.join("home")),
            Some(&root.join("config.toml")),
            Some(&root.join("cache")),
        ))
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env("OMD_CONFIG_PATH", root.join("config.toml"))
        .env("OMD_CACHE_PATH", root.join("cache"))
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

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

fn expected_after_file_observation(store: &mut Store, path: &std::path::Path) -> Expected {
    let mut expected = expected(store);
    let node = expected.basis_versions.keys().next().unwrap().clone();
    let basis = store.read_version(&expected.basis_versions[&node]).unwrap();
    let bytes = std::fs::read(path).unwrap();
    let observed = SourceVersion::new(
        omd::records::ids::Id128::draw(&omd::records::time::OsRng),
        &bytes,
        basis.acquisition,
        basis.encoding,
    );
    let stored = store.persist_observation(&observed, &bytes).unwrap();
    expected
        .source_versions
        .insert(node.clone(), stored.id.to_hex());
    expected
        .acquisition_versions
        .insert(node.clone(), stored.id.to_hex());
    expected.source_hashes.insert(node, stored.sha256);
    expected
}

fn commit_file(
    store: &mut Store,
    node: &str,
    path: &std::path::Path,
    rng: &FixedRng,
    clock: &FixedClock,
) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "path".into(),
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
            .into(),
    );
    let expected = expected(store);
    pipeline::commit_file(
        store,
        &mut NoProbe,
        rng,
        clock,
        node,
        path,
        CommitKind::Init,
        payload,
        &expected,
        None,
    )
    .unwrap()
}

#[test]
fn node_ref_roundtrip_via_display_and_parse() {
    let r = Ref::range("abc123");
    let s = r.to_string();
    assert_eq!(s, "range:abc123");
    let back: Ref = s.parse().unwrap();
    assert_eq!(back, r);
}

#[test]
fn ref_parse_rejects_bad_form() {
    assert!("nocolon".parse::<Ref>().is_err());
    assert!("bogus:id".parse::<Ref>().is_err());
    assert!("range:".parse::<Ref>().is_err());
}

#[test]
fn position_serializes_as_tagged_fields_not_uri() {
    let p = Position::File {
        project: "root".into(),
        path: "docs/a.md".into(),
        encoding: Some("utf-8".into()),
    };
    let t = serde_json::to_value(&p).unwrap();
    assert_eq!(t["type"], "file");
    assert_eq!(t["project"], "root");
    assert_eq!(t["path"], "docs/a.md");
    // No URI — path is a bare field, not a `proj:`/`file://` string.
    assert!(t.get("uri").is_none());
    let c = Position::Command {
        executable: "grep".into(),
        args: vec!["-n".into(), "x".into()],
    };
    let ct = serde_json::to_value(&c).unwrap();
    assert_eq!(ct["type"], "command");
    assert_eq!(ct["args"], serde_json::json!(["-n", "x"]));
    let g = Position::Git {
        project: "upstream".into(),
        commit: "a".repeat(40),
        path: "src/lib.rs".into(),
    };
    let gt = serde_json::to_value(&g).unwrap();
    assert_eq!(gt["type"], "git");
    assert_eq!(gt["project"], "upstream");
    assert!(gt.get("repo").is_none());
    assert!(gt.get("remote").is_none());
}

#[test]
fn refspec_denies_unknown_fields() {
    // A field not in RefSpec must fail deserialization — never silently
    // ignored (typoed flags must error, not pass a half-parsed record).
    let bad = r#"{"kind":"file","id":"x","posn":{"type":"file","path":"a"}}"#;
    assert!(
        toml::from_str::<RefSpec>(bad).is_err() || serde_json::from_str::<RefSpec>(bad).is_err()
    );
    let good = r#"{"kind":"range","id":"r1","range":{"span":[0,5],"unit":"text"}}"#;
    let r: RefSpec = serde_json::from_str(good).unwrap();
    assert_eq!(r.range.unwrap().span, Span(0, 5));
}

#[test]
fn resolve_ref_full_and_unique_prefix() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(1);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "hello").unwrap();
    // First commit = chain root = the node's identity.
    let root_cid = commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    // Second commit on the same chain.
    std::fs::write(&src, "hello2").unwrap();
    let expected = expected_after_file_observation(&mut store, &src);
    let _c2 = pipeline::commit_file(
        &mut store,
        &mut NoProbe,
        &rng,
        &clock,
        "file:a.md",
        &src,
        CommitKind::Commit,
        Default::default(),
        &expected,
        None,
    )
    .unwrap();
    // The node id is the chain root — resolve by full id and by unique prefix.
    for sel in [root_cid.as_str(), &root_cid[..12]] {
        let r = identity::resolve_ref(
            &store,
            &RefSpec {
                kind: Kind::File,
                id: sel.into(),
                position: None,
                range: None,
                store: None,
                version: None,
                expect_version: None,
                link_id: None,
            },
        )
        .unwrap();
        assert_eq!(r.node.id, root_cid);
        assert!(!r.tip.is_empty());
        assert_eq!(r.selected, r.tip);
        assert_eq!(r.effective_range_commit_id, r.tip);
    }
    // A nonexistent id fails; a too-short ambiguous prefix fails.
    let err = identity::resolve_ref(
        &store,
        &RefSpec {
            kind: Kind::File,
            id: "deadbeef".into(),
            position: None,
            range: None,
            store: None,
            version: None,
            expect_version: None,
            link_id: None,
        },
    );
    assert!(err.is_err());
}

#[test]
fn resolve_ref_version_selects_historical_commit() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(2);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "v1").unwrap();
    let c1 = commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    std::fs::write(&src, "v2").unwrap();
    let expected = expected_after_file_observation(&mut store, &src);
    let c2 = pipeline::commit_file(
        &mut store,
        &mut NoProbe,
        &rng,
        &clock,
        "file:a.md",
        &src,
        CommitKind::Commit,
        Default::default(),
        &expected,
        None,
    )
    .unwrap();
    // version selects the historical commit, tip stays current.
    let r = identity::resolve_ref(
        &store,
        &RefSpec {
            kind: Kind::File,
            id: c1.clone(),
            position: None,
            range: None,
            store: None,
            version: Some(c1.clone()),
            expect_version: None,
            link_id: None,
        },
    )
    .unwrap();
    assert_eq!(r.selected, c1);
    assert_eq!(r.tip, c2);
    // A version not on this chain fails.
    let err = identity::resolve_ref(
        &store,
        &RefSpec {
            kind: Kind::File,
            id: c1.clone(),
            position: None,
            range: None,
            store: None,
            version: Some("0".repeat(64)),
            expect_version: None,
            link_id: None,
        },
    );
    assert!(err.is_err());
}

#[test]
fn resolve_ref_expect_version_guards_effective_tip() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(3);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "v1").unwrap();
    let c1 = commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    // expect_version matching the current tip passes.
    let ok = identity::resolve_ref(
        &store,
        &RefSpec {
            kind: Kind::File,
            id: c1.clone(),
            position: None,
            range: None,
            store: None,
            version: None,
            expect_version: Some(c1.clone()),
            link_id: None,
        },
    );
    assert!(ok.is_ok());
    // Mismatched expectation fails.
    let err = identity::resolve_ref(
        &store,
        &RefSpec {
            kind: Kind::File,
            id: c1,
            position: None,
            range: None,
            store: None,
            version: None,
            expect_version: Some("1".repeat(64)),
            link_id: None,
        },
    );
    assert!(matches!(
        err,
        Err(identity::RefError::VersionMismatch(_, _))
    ));
}

#[test]
fn is_marker_commit_flags_atomic_boundaries() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(4);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "x").unwrap();
    let _init = commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    // Open a block — the BEGIN commit is a marker.
    let expected = expected(&store);
    let begin = pipeline::commit_marker(
        &mut store,
        &mut NoProbe,
        &rng,
        &clock,
        "file:a.md",
        CommitKind::AtomicBegin,
        Default::default(),
        &expected,
    )
    .unwrap();
    let file_node = identity::Ref::file(_init.clone()).key();
    assert!(identity::is_marker_commit(&store, &file_node, &begin));
    assert!(!identity::is_marker_commit(&store, &file_node, &_init));
}

#[test]
fn resolver_rejects_ambiguous_commit_prefix() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(9);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "v1").unwrap();
    commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    std::fs::write(&src, "v2").unwrap();
    let expected = expected_after_file_observation(&mut store, &src);
    pipeline::commit_file(
        &mut store,
        &mut NoProbe,
        &rng,
        &clock,
        "file:a.md",
        &src,
        CommitKind::Commit,
        Default::default(),
        &expected,
        None,
    )
    .unwrap();
    assert!(matches!(
        identity::resolve_commit_id(&store, ""),
        Err(identity::RefError::AmbiguousVersion(_))
    ));
}

#[test]
fn tampered_location_index_is_rejected_untouched() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(10);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "x").unwrap();
    commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    let state_path = store.root().join("state.toml");
    let tampered = std::fs::read_to_string(&state_path)
        .unwrap()
        .replace("a.md", "wrong.md");
    std::fs::write(&state_path, tampered.as_bytes()).unwrap();
    drop(store);
    let before = std::fs::read(&state_path).unwrap();
    assert!(Store::open(&dir.path().join("omd")).is_err());
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
}

#[test]
fn missing_immutable_predecessor_is_rejected_untouched() {
    let (dir, mut store) = fresh_store();
    let rng = FixedRng::new(11);
    let clock = FixedClock::new(Timestamp(1_700_000_000_000_000_000));
    let src = dir.path().join("a.md");
    std::fs::write(&src, "v1").unwrap();
    let root = commit_file(&mut store, "file:a.md", &src, &rng, &clock);
    std::fs::write(&src, "v2").unwrap();
    let expected = expected_after_file_observation(&mut store, &src);
    pipeline::commit_file(
        &mut store,
        &mut NoProbe,
        &rng,
        &clock,
        "file:a.md",
        &src,
        CommitKind::Commit,
        Default::default(),
        &expected,
        None,
    )
    .unwrap();
    std::fs::remove_file(store.root().join(format!("commits/{root}.toml"))).unwrap();
    let state_path = store.root().join("state.toml");
    let before = std::fs::read(&state_path).unwrap();
    drop(store);
    assert!(Store::open(&dir.path().join("omd")).is_err());
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
}

#[test]
fn mutable_projection_cannot_retype_file_chain_as_range() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("home")).unwrap();
    std::fs::write(dir.path().join("a.md"), "abcdef").unwrap();
    std::fs::write(dir.path().join("b.md"), "abcdef").unwrap();
    assert_eq!(run_cli(dir.path(), &["init", "a.md"]).0, 0);
    assert_eq!(run_cli(dir.path(), &["init", "b.md"]).0, 0);

    let state_path = dir.path().join(".omd/state.toml");
    let mut state: omd::records::store::State =
        toml::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    let file_a = omd::relations::node::file_at_path(&state, "a.md")
        .unwrap()
        .to_string();
    let file_b = omd::relations::node::file_at_path(&state, "b.md")
        .unwrap()
        .to_string();
    let tip_a = state.tips.remove(&file_a).unwrap();
    let range_a = format!("range:{}", file_a.strip_prefix("file:").unwrap());
    state.tips.insert(range_a.clone(), tip_a);
    state.locations.remove(&file_a);
    state
        .mounts
        .entry(file_b)
        .or_default()
        .push(range_a.clone());
    std::fs::write(&state_path, toml::to_string(&state).unwrap()).unwrap();
    let before = std::fs::read(&state_path).unwrap();
    let commits_before = std::fs::read_dir(dir.path().join(".omd/commits"))
        .unwrap()
        .count();

    let (list_code, _, list_err) = run_cli(dir.path(), &["list"]);
    assert_ne!(list_code, 0, "tampered projection accepted: {list_err}");
    let (commit_code, _, commit_err) = run_cli(
        dir.path(),
        &[
            "commit",
            "commit",
            "b.md",
            "--range",
            "0",
            "2",
            "--link-from",
            &range_a,
            "--reason",
            "test",
        ],
    );
    assert_ne!(
        commit_code, 0,
        "retyped file accepted as range: {commit_err}"
    );
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(dir.path().join(".omd/commits"))
            .unwrap()
            .count(),
        commits_before
    );
}

#[test]
fn immediately_previous_store_and_commit_formats_are_refused_untouched() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "abc").unwrap();
    let (code, out, err) = run_cli(dir.path(), &["init", "a.md"]);
    assert_eq!(code, 0, "init: {out} {err}");
    let omd_dir = dir.path().join(".omd");

    let state_path = omd_dir.join("state.toml");
    let current_state = std::fs::read_to_string(&state_path).unwrap();
    let old_state = current_state.replace("omd.state/9", "omd.state/8");
    std::fs::write(&state_path, &old_state).unwrap();
    let before_state = std::fs::read(&state_path).unwrap();
    let open = Store::open_existing(&omd_dir);
    assert!(open.is_err(), "omd.state/8 must be refused");
    assert_eq!(std::fs::read(&state_path).unwrap(), before_state);

    std::fs::write(&state_path, current_state).unwrap();
    let commit_id = std::fs::read_dir(omd_dir.join("commits"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .trim_end_matches(".toml")
        .to_string();
    let commit_path = omd_dir.join(format!("commits/{commit_id}.toml"));
    let old_commit = std::fs::read_to_string(&commit_path)
        .unwrap()
        .replace("omd.commit/3", "omd.commit/2");
    std::fs::write(&commit_path, &old_commit).unwrap();
    let before_commit = std::fs::read(&commit_path).unwrap();
    let read = toml::from_str::<omd::records::commit::Commit>(&old_commit)
        .unwrap()
        .validate(true);
    assert!(read.is_err(), "omd.commit/2 must be refused");
    assert_eq!(std::fs::read(&commit_path).unwrap(), before_commit);
}

// 1.1: a store written by the coordinate-key generation is refused at open
// — never migrated, and its directory is left byte-identical.
#[test]
fn legacy_store_refused_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let omd_dir = dir.path().join("omd");
    std::fs::create_dir_all(&omd_dir).unwrap();
    // A legacy state: coordinate-keyed tips, no `format` field.
    let legacy = r#"publication = 3
[tips]
"file:a.md" = "aaaa1111"
"range:a.md@text:0-3" = "bbbb2222"
"#;
    std::fs::write(omd_dir.join("state.toml"), legacy).unwrap();
    // Snapshot dir bytes before open.
    let before: std::collections::BTreeMap<_, _> = std::fs::read_dir(&omd_dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (e.file_name(), std::fs::read(e.path()).unwrap_or_default())
        })
        .collect();
    // Open must refuse — not migrate, not reinterpret the coordinate keys.
    let r = Store::open(&omd_dir);
    assert!(r.is_err(), "legacy store refused: {:?}", r.map(|_| ()));
    let err = format!("{:?}", r.err().unwrap());
    assert!(
        err.contains("omd.state/1") || err.contains("unsupported"),
        "format named: {err}"
    );
    // Directory bytes unchanged — refusal leaves no residue.
    let after: std::collections::BTreeMap<_, _> = std::fs::read_dir(&omd_dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (e.file_name(), std::fs::read(e.path()).unwrap_or_default())
        })
        .collect();
    assert_eq!(before, after, "refused open left the dir untouched");
}
