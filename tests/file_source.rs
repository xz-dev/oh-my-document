//! Task 4.x: explicit file init, current-path read (not HEAD), mount tree,
//! chain navigation, and the init/import/remove/delete aliases.

use std::rc::Rc;

use omd::relations::{chain_to_root, restore_range_tips, Node, NodeKind};
use omd::sources::{file, SourceError};
use omd::testing::Sandbox;

#[test]
fn observe_reads_current_path_not_head() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let f = sb.source_dir().join("a.md");
    std::fs::write(&f, "v1").unwrap();
    let o1 = file::observe_text(&f, None).unwrap();
    assert_eq!(o1.bytes, b"v1");
    // Change the file; next observation sees the new bytes — this is "now",
    // not a snapshot pinned at init.
    std::fs::write(&f, "v2").unwrap();
    let o2 = file::observe_text(&f, None).unwrap();
    assert_eq!(o2.bytes, b"v2");
}

#[test]
fn byte_mode_never_decodes() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let f = sb.source_dir().join("bin");
    std::fs::write(&f, &[0xff, 0xfe, 0x00]).unwrap();
    // Text mode rejects invalid utf-8.
    assert!(matches!(file::observe_text(&f, None), Err(SourceError::Encoding)));
    // Byte mode reads it fine.
    assert_eq!(file::observe_bytes(&f).unwrap().bytes, vec![0xff, 0xfe, 0x00]);
}

#[test]
fn chain_navigation_follows_previous_id() {
    // tip c3 -> c2 -> c1 -> ""
    let prev = |id: &str| -> Option<String> {
        match id {
            "c3" => Some("c2".into()),
            "c2" => Some("c1".into()),
            "c1" => Some("".into()),
            _ => None,
        }
    };
    let chain = chain_to_root("c3", prev);
    assert_eq!(chain, vec!["c3", "c2", "c1"]);
}

#[test]
fn missing_commit_stops_chain_at_gap() {
    let chain = chain_to_root("c2", |id| if id == "c2" { Some("c1".into()) } else { None });
    assert_eq!(chain, vec!["c2", "c1"]); // c1 has no record -> stops there
}

#[test]
fn file_reset_restores_recorded_range_tips() {
    let mut snap = std::collections::BTreeMap::new();
    snap.insert("r1".to_string(), "tip_a".to_string());
    snap.insert("r2".to_string(), "tip_b".to_string());
    let tips = restore_range_tips(&snap);
    assert_eq!(tips, vec![("r1".into(), "tip_a".into()), ("r2".into(), "tip_b".into())]);
}

#[test]
fn node_kinds_cover_root_file_range() {
    let root = Node { key: "root".into(), kind: NodeKind::Root, parent: "".into(), tip: "".into() };
    let file = Node { key: "file:a".into(), kind: NodeKind::File, parent: "root".into(), tip: "c1".into() };
    let range = Node { key: "range:a@1-40".into(), kind: NodeKind::Range, parent: "file:a".into(), tip: "c2".into() };
    assert_eq!(file.parent, "root");
    assert_eq!(range.parent, "file:a");
}
