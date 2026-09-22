//! Reset semantics: tip move, dangling, link/adapt withdrawal, JSON outcome.

mod common;
use std::process::Command;
static TDIR_UNIQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn omd() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") {
        return p.into();
    }
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    p.pop();
    p.push("omd");
    p
}

struct T(std::path::PathBuf);
impl T {
    fn file_node(&self, path: &str) -> String {
        let value: toml::Value = toml::from_str(&self.state()).unwrap();
        value
            .get("locations")
            .and_then(toml::Value::as_table)
            .and_then(|locations| {
                locations.iter().find_map(|(node, current)| {
                    (current.as_str() == Some(path)).then(|| node.clone())
                })
            })
            .unwrap_or_default()
    }
    /// The N-th range node key mounted under the file object at `file`.
    fn range_node(&self, file: &str, n: usize) -> String {
        let s = self.state();
        let file_node = self.file_node(file);
        s.lines()
            .find(|l| l.contains(&format!("\"{file_node}\"")) && l.contains('['))
            .and_then(|l| {
                l.match_indices("range:").nth(n).and_then(|(i, _)| {
                    let r = &l[i..];
                    r.find('"').map(|e| r[..e].to_string())
                })
            })
            .unwrap_or_default()
    }
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!(
            "omd-rs-{}-{}",
            std::process::id(),
            TDIR_UNIQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&r).unwrap();
        Self(r)
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = Command::new(omd())
            .arg("--meta")
            .arg(self.0.join(".omd"))
            .args(common::with_expected(
                &omd(),
                &self.0,
                args,
                Some(&self.0.join(".omd")),
                Some(&self.0.join("home")),
                Some(&self.0.join("config.toml")),
                Some(&self.0.join("cache")),
            ))
            .current_dir(&self.0)
            .env("HOME", self.0.join("home"))
            .env("OMD_CONFIG_PATH", self.0.join("config.toml"))
            .env("OMD_CACHE_PATH", self.0.join("cache"))
            .output()
            .unwrap();
        (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).into(),
            String::from_utf8_lossy(&o.stderr).into(),
        )
    }
    fn commits_snapshot(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(self.0.join(".omd/commits"))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }
    fn authority_snapshot(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        fn collect(
            root: &std::path::Path,
            dir: &std::path::Path,
            files: &mut std::collections::BTreeMap<String, Vec<u8>>,
        ) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    collect(root, &entry.path(), files);
                } else {
                    files.insert(
                        entry
                            .path()
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into(),
                        std::fs::read(entry.path()).unwrap(),
                    );
                }
            }
        }
        let root = self.0.join(".omd");
        let mut files = std::collections::BTreeMap::new();
        collect(&root, &root, &mut files);
        files
    }
    fn write(&self, p: &str, c: &str) {
        std::fs::write(self.0.join(p), c).unwrap();
    }
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
    fn tip(&self, node: &str) -> String {
        let s = self.state();
        // `range:<file>@<span>` (test spelling) → resolve the mounted range
        // node under `file:<file>` whose tip commit payload records the span.
        // Identity is the chain root, never the coordinates — the span is
        // only a selector for *which* mounted chain the test means.
        if let Some(rest) = node.strip_prefix("range:") {
            if let Some(at) = rest.find('@') {
                let path = &rest[..at];
                let want = rest[at + 1..]
                    .strip_prefix("text:")
                    .unwrap_or(&rest[at + 1..]);
                let file_node = self.file_node(path);
                for l in s.lines() {
                    if l.contains(&format!("\"{file_node}\"")) && l.contains('[') {
                        for m in l.match_indices("range:") {
                            let r = &l[m.0..];
                            if let Some(e) = r.find('"') {
                                let key = &r[..e];
                                let tip_id = s
                                    .lines()
                                    .find(|x| {
                                        x.starts_with(&format!("\"{key}\"")) && x.contains('=')
                                    })
                                    .and_then(|x| x.split('=').nth(1))
                                    .map(|v| v.trim().trim_matches('"').to_string())
                                    .unwrap_or_default();
                                if tip_id.is_empty() {
                                    continue;
                                }
                                let cm = std::fs::read_to_string(
                                    self.0.join(format!(".omd/commits/{tip_id}.toml")),
                                )
                                .unwrap_or_default();
                                let span = cm
                                    .lines()
                                    .find(|x| x.contains("range"))
                                    .and_then(|x| x.split('"').nth(1))
                                    .unwrap_or("")
                                    .to_string();
                                let span_norm =
                                    span.strip_prefix("text:").unwrap_or(&span).to_string();
                                if span_norm == want {
                                    return tip_id;
                                }
                            }
                        }
                    }
                }
                // No span match — the chain's extent moved; take the first
                // mounted child (tests name the chain, not the coordinates).
                for l in s.lines() {
                    if l.contains(&format!("\"{file_node}\"")) && l.contains('[') {
                        if let Some(m) = l.match_indices("range:").next() {
                            let r = &l[m.0..];
                            if let Some(e) = r.find('"') {
                                let key = &r[..e];
                                return s
                                    .lines()
                                    .find(|x| {
                                        x.starts_with(&format!("\"{key}\"")) && x.contains('=')
                                    })
                                    .and_then(|x| x.split('=').nth(1))
                                    .map(|v| v.trim().trim_matches('"').to_string())
                                    .unwrap_or_default();
                            }
                        }
                    }
                }
                return String::new();
            }
        }
        let node = node
            .strip_prefix("file:")
            .and_then(|path| {
                (!path.chars().all(|c| c.is_ascii_hexdigit())).then(|| self.file_node(path))
            })
            .filter(|node| !node.is_empty())
            .unwrap_or_else(|| node.to_string());
        s.lines()
            .find(|l| l.contains(&format!("\"{node}\"")) && l.contains('='))
            .and_then(|l| {
                l.split('=')
                    .nth(1)
                    .map(|v| v.trim().trim_matches('"').to_string())
            })
            .unwrap_or_default()
    }
    fn prev(&self, cid: &str) -> String {
        std::fs::read_to_string(self.0.join(format!(".omd/commits/{cid}.toml")))
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("previous_id"))
            .map(|l| {
                l.split('=')
                    .nth(1)
                    .unwrap()
                    .trim()
                    .trim_matches('"')
                    .to_string()
            })
            .unwrap_or_default()
    }
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mk_range(t: &T) -> String {
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r",
    ]);
    t.tip("range:a.md@text:0-1")
}

#[test]
fn file_reset_restores_saved_child_tip_after_later_revision() {
    let t = T::new();
    t.write("a.md", "0123456789");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--range", "0", "3", "--reason", "r0",
        ])
        .0,
        0
    );
    let range_node = t.range_node("a.md", 0);
    let saved_child_tip = t.tip(&range_node);
    let (verify_code, _, verify_err) = t.run(&["commit", "verify", "a.md"]);
    assert_eq!(verify_code, 0, "snapshot file commit failed: {verify_err}");
    let file_snapshot = t.tip("file:a.md");
    assert_eq!(
        t.run(&[
            "commit",
            "commit",
            "a.md",
            "--id",
            &saved_child_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "later",
        ])
        .0,
        0
    );
    let later_child_tip = t.tip(&range_node);
    assert_ne!(later_child_tip, saved_child_tip, "range must advance");

    let (code, out, err) = t.run(&["commit", "reset", "a.md", "--reset-target", &file_snapshot]);
    assert_eq!(code, 0, "file reset failed: {out} {err}");
    assert_eq!(
        t.tip(&range_node),
        saved_child_tip,
        "file reset must restore exact saved child tip"
    );
    let (_, dangling, _) = t.run(&["list", "--dangling"]);
    assert!(
        dangling.contains(&later_child_tip),
        "later child revision must dangle: {dangling}"
    );
}

#[test]
fn file_reset_before_rename_rebuilds_location_and_rejects_collision_atomically() {
    let t = T::new();
    t.write("a.md", "old");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    let old_file_node = t.file_node("a.md");
    let before_rename = t.tip(&old_file_node);
    assert_eq!(t.run(&["rename", "a.md", "b.md"]).0, 0);
    std::fs::rename(t.0.join("a.md"), t.0.join("b.md")).unwrap();
    t.write("a.md", "new");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    let state_before = t.state();
    let history_before = t.commits_snapshot();

    let (code, out, err) = t.run(&["commit", "reset", "b.md", "--reset-target", &before_rename]);
    assert_ne!(code, 0, "occupied restored path accepted: {out} {err}");
    assert_eq!(t.state(), state_before, "failed reset changed state");
    assert_eq!(
        t.commits_snapshot(),
        history_before,
        "failed reset changed immutable history"
    );

    std::fs::remove_file(t.0.join("a.md")).unwrap();
    assert_eq!(t.run(&["delete", "a.md"]).0, 0);
    let (code, out, err) = t.run(&["commit", "reset", "b.md", "--reset-target", &before_rename]);
    assert_eq!(code, 0, "unoccupied restored path rejected: {out} {err}");
    assert_eq!(t.file_node("a.md"), old_file_node);
    assert_eq!(t.run(&["list"]).0, 0, "store must reopen after reset");
    assert!(t.0.join("b.md").exists(), "reset must not move user file");
}

#[test]
fn file_reset_rejects_parent_file_block_member_before_restoring_sibling() {
    let t = T::new();
    let commit = |args: &[&str]| -> String {
        let (code, out, err) = t.run(args);
        assert_eq!(code, 0, "commit failed: {out} {err}");
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["data"]["commit"]
            .as_str()
            .unwrap()
            .to_string()
    };

    t.write("a.md", "abcdef");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    let r0 = commit(&[
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r0", "--json",
    ]);
    let sibling_saved = commit(&[
        "commit", "commit", "a.md", "--range", "3", "4", "--reason", "sibling", "--json",
    ]);
    let blocked_child = t.range_node("a.md", 0);
    let sibling = t.range_node("a.md", 1);
    let block_begin = commit(&["commit", "atomic_begin", "a.md", "--json"]);
    let blocked_saved = commit(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &r0,
        "--range",
        "0",
        "2",
        "--reason",
        "inside-file-block",
        "--json",
    ]);
    commit(&["commit", "atomic_end", "a.md", "--json"]);
    let file_snapshot = commit(&["commit", "verify", "a.md", "--json"]);
    let blocked_later = commit(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &blocked_saved,
        "--range",
        "0",
        "3",
        "--reason",
        "later",
        "--json",
    ]);
    let sibling_later = commit(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &sibling_saved,
        "--range",
        "3",
        "5",
        "--reason",
        "sibling-later",
        "--json",
    ]);
    let state_before: toml::Value = toml::from_str(&t.state()).unwrap();
    let publication_before = state_before["publication"].as_integer().unwrap();
    let commits_before = t.commits_snapshot().len();
    let inbound_before = state_before
        .get("inbound")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default();
    let authority_before = t.authority_snapshot();

    let (code, out, err) = t.run(&["commit", "reset", "a.md", "--reset-target", &file_snapshot]);
    assert_eq!(code, 2, "parent-file block member accepted: {out} {err}");
    let diagnostic = format!("{out}{err}");
    assert!(
        diagnostic.contains(&blocked_child),
        "child missing: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&blocked_saved),
        "target missing: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&block_begin),
        "block basis missing: {diagnostic}"
    );
    assert!(
        diagnostic.contains("parent-file"),
        "block basis unclear: {diagnostic}"
    );
    assert_eq!(t.tip(&blocked_child), blocked_later);
    assert_eq!(t.tip(&sibling), sibling_later);
    assert_eq!(t.authority_snapshot(), authority_before);
    let state_after: toml::Value = toml::from_str(&t.state()).unwrap();
    assert_eq!(
        state_after["publication"].as_integer(),
        Some(publication_before)
    );
    assert_eq!(t.commits_snapshot().len(), commits_before);
    assert_eq!(
        state_after
            .get("inbound")
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default(),
        inbound_before
    );
}

#[test]
fn file_reset_restores_saved_child_end_exactly() {
    let t = T::new();
    t.write("a.md", "abc");
    t.write("b.md", "xyz");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    assert_eq!(t.run(&["init", "b.md"]).0, 0);
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--range", "0", "1", "--reason", "source",
        ])
        .0,
        0
    );
    let source = t.range_node("a.md", 0);
    assert_eq!(
        t.run(&[
            "commit",
            "commit",
            "b.md",
            "--range",
            "0",
            "1",
            "--link-from",
            &source,
            "--reason",
            "combo",
        ])
        .0,
        0
    );
    let child = t.range_node("b.md", 0);
    let saved_end = t.tip(&child);
    let saved_commit =
        std::fs::read_to_string(t.0.join(format!(".omd/commits/{saved_end}.toml"))).unwrap();
    assert!(saved_commit.contains("atomic_end"), "fixture must save END");
    assert_eq!(t.run(&["commit", "verify", "b.md"]).0, 0);
    let file_snapshot = t.tip("file:b.md");
    assert_eq!(
        t.run(&[
            "commit", "commit", "b.md", "--id", &saved_end, "--range", "0", "2", "--reason",
            "later",
        ])
        .0,
        0
    );
    assert_ne!(t.tip(&child), saved_end);
    let (code, out, err) = t.run(&["commit", "reset", "b.md", "--reset-target", &file_snapshot]);
    assert_eq!(code, 0, "file reset failed: {out} {err}");
    assert_eq!(t.tip(&child), saved_end, "saved END shifted during restore");
}

#[test]
fn file_reset_rejects_wrong_kind_child_target_without_publication() {
    let t = T::new();
    t.write("a.md", "0123456789");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    let file_init = t.tip("file:a.md");
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--range", "0", "3", "--reason", "range",
        ])
        .0,
        0
    );
    let child = t.range_node("a.md", 0);
    assert_eq!(t.run(&["commit", "verify", "a.md"]).0, 0);
    let snapshot = t.tip("file:a.md");
    let snapshot_path = t.0.join(format!(".omd/commits/{snapshot}.toml"));
    let mut record: omd::records::commit::Commit =
        toml::from_str(&std::fs::read_to_string(&snapshot_path).unwrap()).unwrap();
    record.range_tips.insert(child, file_init);
    std::fs::write(&snapshot_path, toml::to_string(&record).unwrap()).unwrap();
    let state_before = t.state();
    let history_before = t.commits_snapshot();

    let (code, out, err) = t.run(&["commit", "reset", "a.md", "--reset-target", &snapshot]);
    assert_ne!(code, 0, "wrong-kind child target accepted: {out} {err}");
    assert_eq!(t.state(), state_before);
    assert_eq!(t.commits_snapshot(), history_before);
}

#[test]
fn reset_moves_tip_to_landing_and_dangles_removed() {
    let t = T::new();
    let c1 = mk_range(&t);
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "c2",
    ]);
    let c2 = t.tip("range:a.md@text:0-1");
    assert_ne!(c1, c2);
    // Reset to c1.
    t.run(&["commit", "reset", "a.md", "--reset-target", &c1]);
    // The new tip is the reset marker; its previous is c1 — c2 is unreachable.
    let tip = t.tip("range:a.md@text:0-1");
    assert_eq!(t.prev(&tip), c1, "reset marker must chain onto landing c1");
    // c2 must be dangling.
    let (_, out, _) = t.run(&["list", "--dangling"]);
    assert!(out.contains(&c2), "c2 must dangle: {out}");
}

#[test]
fn reset_reports_requested_actual_in_json() {
    let t = T::new();
    let c1 = mk_range(&t);
    let (_, out, _) = t.run(&["commit", "reset", "a.md", "--reset-target", &c1, "--json"]);
    assert!(
        out.contains("\"requested\""),
        "must report requested: {out}"
    );
    assert!(out.contains("\"actual\""), "must report actual: {out}");
}

#[test]
fn reset_boundary_warns_and_lands_on_predecessor() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    // BEGIN/END block (markers take no --reason): reset to END lands on
    // BEGIN's predecessor.
    t.run(&["commit", "atomic_begin", "a.md"]);
    let begin = t.tip("file:a.md");
    t.run(&["commit", "atomic_end", "a.md"]);
    let end = t.tip("file:a.md");
    assert_ne!(begin, end);
    let (_, out, _) = t.run(&["commit", "reset", "a.md", "--reset-target", &end, "--json"]);
    // Boundary reset carries a warning naming the landing.
    assert!(out.contains("warning"), "boundary reset must warn: {out}");
}

#[test]
fn reset_interior_member_refused() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "atomic_begin", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--reason", "inside"]);
    let inside = t.tip("file:a.md");
    // An ordinary commit inside an open block is not a reset target.
    let (c, out, e) = t.run(&["commit", "reset", "a.md", "--reset-target", &inside]);
    assert!(
        c != 0 || e.contains("member") || e.contains("Interior") || out.contains("block member"),
        "interior member must refuse: {out} {e}"
    );
}

#[test]
fn new_commit_after_reset_continues_chain() {
    let t = T::new();
    let c1 = mk_range(&t);
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "c2",
    ]);
    t.run(&["commit", "reset", "a.md", "--reset-target", &c1]);
    // A new commit after reset continues from the reset tip.
    let (c, _, _) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "fresh",
    ]);
    assert_eq!(c, 0, "commit after reset must succeed");
}

// Re-audit BUG1: reset to BEGIN withdraws links created in the removed
// segment — the link must not survive its creating commit's removal.
#[test]
fn reset_to_begin_withdraws_link_created_in_segment() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // BEGIN on b's range, then a LINK commit inside, then END.
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &t.range_node("a.md", 0),
        "--reason",
        "lb",
    ]);
    let st = t.state();
    assert!(st.contains("[links."), "link created: {st}");
    // Find the block's BEGIN commit = the chain member whose previous_id is
    // empty (the block root) under range:b.md.
    let begin = std::fs::read_dir(t.0.join(".omd/commits"))
        .unwrap()
        .flatten()
        .find_map(|e| {
            let txt = std::fs::read_to_string(e.path()).ok()?;
            if txt.contains("kind = \"atomic_begin\"") {
                Some(e.path().file_stem().unwrap().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .expect("a BEGIN commit exists");
    // Reset b's range to the block BEGIN — removed segment = commit+link+end.
    let (c, o, e) = t.run(&["commit", "reset", "b.md", "--reset-target", &begin]);
    assert_eq!(c, 0, "{o} {e}");
    let st2 = t.state();
    // The link created in the removed segment is withdrawn.
    assert!(
        !st2.contains("[links."),
        "link withdrawn after reset: {st2}"
    );
}
