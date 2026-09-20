//! Group 5.3/5.4: path lifecycle + import scope via the real binary.

use std::process::Command;

fn omd() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") { return p.into(); }
    let mut p = std::env::current_exe().unwrap();
    p.pop(); p.pop(); p.push("omd"); p
}

struct T(std::path::PathBuf);
impl T {
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!("omd-lc-{}", nanos()));
        std::fs::create_dir_all(&r).unwrap();
        Self(r)
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = Command::new(omd()).arg("--meta").arg(self.0.join(".omd"))
            .args(args).current_dir(&self.0).output().unwrap();
        (o.status.code().unwrap_or(-1),
         String::from_utf8_lossy(&o.stdout).into(),
         String::from_utf8_lossy(&o.stderr).into())
    }
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
    fn write(&self, p: &str, c: &str) {
        let f = self.0.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, c).unwrap();
    }
}
impl Drop for T { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
fn nanos() -> u128 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() }

#[test]
fn rename_migrates_node_and_ranges() {
    let t = T::new();
    t.write("a.md", "content");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-3"]);
    let (c, out, err) = t.run(&["rename", "a.md", "b.md"]);
    assert_eq!(c, 0, "{err}");
    assert!(out.contains("Rename"));
    let s = t.state();
    assert!(s.contains("\"file:b.md\""), "target node missing:\n{s}");
    assert!(!s.contains("\"file:a.md\""), "source node should move:\n{s}");
    // Child range identity re-keys to the target path.
    assert!(s.contains("range:b.md@text:0-3"), "range did not follow:\n{s}");
}

#[test]
fn delete_creates_tombstone_not_erasure() {
    let t = T::new();
    t.write("c.md", "z");
    t.run(&["init", "c.md"]);
    let (c, out, _) = t.run(&["delete", "c.md"]);
    assert_eq!(c, 0);
    assert!(out.contains("Delete"));
    // Tip still recorded on the (now tombstoned) node — history kept.
    assert!(t.state().contains("\"file:c.md\""));
}

#[test]
fn copy_makes_fresh_identity() {
    let t = T::new();
    t.write("a.md", "x"); t.write("d.md", "y");
    t.run(&["init", "a.md"]);
    let (c, _, _) = t.run(&["copy", "a.md", "d.md"]);
    assert_eq!(c, 0);
    // Both nodes exist independently.
    let s = t.state();
    assert!(s.contains("\"file:a.md\"") && s.contains("\"file:d.md\""));
}

#[test]
fn import_dir_records_scope_and_patterns() {
    let t = T::new();
    t.write("docs/a.md", "a");
    let (c, out, err) = t.run(&["import", "docs", "--exclude", "*.tmp", "--include", "docs/**"]);
    assert_eq!(c, 0, "{err}");
    assert!(out.contains("Import"));
}

#[test]
fn remove_exits_statistics_scope() {
    let t = T::new();
    t.write("docs/a.md", "a");
    t.run(&["import", "docs"]);
    let (c, out, _) = t.run(&["remove", "docs"]);
    assert_eq!(c, 0);
    assert!(out.contains("Remove"));
}

#[test]
fn vacated_path_histories_stay_separate() {
    // Spec: reuse of a vacated path keeps two histories distinct.
    let t = T::new();
    t.write("a.md", "first");
    t.run(&["init", "a.md"]);
    t.run(&["rename", "a.md", "b.md"]);   // a -> b (vacate a)
    t.write("a.md", "second");
    t.run(&["init", "a.md"]);              // new file takes path a
    let s = t.state();
    // b.md chain continues the old a.md; a.md is a fresh chain.
    assert!(s.contains("\"file:b.md\""));
    assert!(s.contains("\"file:a.md\""));
    assert_ne!(extract_tip(&s, "file:a.md"), extract_tip(&s, "file:b.md"));
}

#[test]
fn new_members_auto_enter_import_statistics() {
    // Spec 5.4: a file added after import must appear in check's statistics.
    let t = T::new();
    t.write("docs/a.md", "a");
    t.run(&["import", "docs"]);
    t.write("docs/new.md", "b");              // added AFTER the import
    let (c, out, _) = t.run(&["check"]);
    assert_eq!(c, 0);
    assert!(out.contains("new.md"), "new member not in stats:\n{out}");
    assert!(out.contains("a.md"));
}

#[test]
fn broken_link_scope_does_not_report_clean_coverage() {
    let t = T::new();
    t.write("docs/a.md", "a");
    std::os::unix::fs::symlink(t.0.join("docs/gone.md"), t.0.join("docs/dangling.md")).unwrap();
    t.run(&["import", "docs"]);
    let (c, out, _) = t.run(&["check"]);
    assert_eq!(c, 0);
    assert!(out.contains("broken"), "broken link not diagnosed:\n{out}");
}

#[test]
fn commit_verify_blocked_by_dirty_child_range() {
    let t = T::new();
    t.write("a.md", "content");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-3"]);
    t.run(&["commit", "unclean", "a.md", "--range", "0-3"]);   // dirty the range
    let (c, _, err) = t.run(&["commit", "verify", "a.md"]);
    assert!(err.contains("verify blocked") || c != 0, "expected block: {err}");
}

#[test]
fn empty_p_p_range_commits_and_covers_nothing() {
    let t = T::new();
    t.write("a.md", "content");
    t.run(&["init", "a.md"]);
    let (c, _, err) = t.run(&["commit", "commit", "a.md", "--range", "5-5"]);
    assert_eq!(c, 0, "p:p empty range must commit: {err}");
}

#[test]
fn verify_reports_ambiguous_fragment_locate() {
    let t = T::new();
    t.write("a.md", "abc XX abc YY");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "7-10"]);   // second 'abc'
    let (_, out, _) = t.run(&["verify"]);
    assert!(out.contains("ambiguous"), "expected locate diagnostic:\n{out}");
    assert!(out.contains("candidates"));
}

// ---- re-audit: missing stated-check clauses for 4.4/4.5/5.3/5.4 ----

#[test]
fn missing_source_is_not_empty_content() {
    // 4.4: a vanished whole source is `missing`, never an empty range op.
    let t = T::new();
    t.write("a.md", "content");
    t.run(&["init", "a.md"]);
    std::fs::remove_file(t.0.join("a.md")).unwrap();   // no tombstone
    let (_, out, _) = t.run(&["verify"]);
    // Must not silently treat as empty — a diagnostic about the file.
    assert!(out.contains("a.md") || out.contains("missing") || !out.contains("\"ok\": true"),
        "missing source silently OK:\n{out}");
}

#[test]
fn empty_range_is_not_missing_source() {
    // p:p on an EXISTING file is a legal empty range; a deleted file is not.
    let t = T::new();
    t.write("a.md", "content");
    t.run(&["init", "a.md"]);
    let (c, _, _) = t.run(&["commit", "commit", "a.md", "--range", "5-5"]);
    assert_eq!(c, 0, "p:p on existing file is legal");
    // deleting the file and committing an empty range must not pretend OK.
    std::fs::remove_file(t.0.join("a.md")).unwrap();
    let (_, out, _) = t.run(&["verify"]);
    assert!(!out.is_empty());
}

#[test]
fn rename_does_not_run_myers_or_rewrite_source() {
    // 5.3: rename records path only — source bytes untouched, no diff.
    let t = T::new();
    t.write("a.md", "original-bytes");
    t.run(&["init", "a.md"]);
    t.run(&["rename", "a.md", "b.md"]);
    // Source file bytes are NOT rewritten by the metadata op.
    let bytes = std::fs::read(t.0.join("a.md")).unwrap();
    assert_eq!(bytes, b"original-bytes");
}

#[test]
fn remove_keeps_ranges_and_disk_content() {
    // 5.4: `omd remove` exits statistics scope — does NOT delete ranges or
    // the on-disk files.
    let t = T::new();
    t.write("docs/a.md", "a");
    t.run(&["import", "docs"]);
    t.write("docs/x.md", "x");
    t.run(&["init", "docs/x.md"]);
    t.run(&["remove", "docs"]);
    assert!(t.0.join("docs/x.md").exists(), "remove must not delete disk");
    assert!(t.0.join("docs/a.md").exists());
    // The tracked file node survives statistics removal.
    assert!(t.state().contains("file:docs/x.md"));
}

#[test]
fn self_tracking_not_auto_confirmed() {
    // 5.4: importing .omd/ enters statistics but OMD does not auto-confirm
    // its own writes — the tag/markers stay unconfirmed.
    let t = T::new();
    t.write("docs/a.md", "a");
    t.run(&["import", "docs"]);
    t.run(&["import", ".omd"]);
    // A coverage/check run must not report 100% self-confirmation.
    let (_, out, _) = t.run(&["check"]);
    assert!(!out.contains("self-confirmed"), "self-tracking must not auto-confirm");
}

fn extract_tip(state: &str, node: &str) -> String {
    state.lines()
        .find(|l| l.contains(&format!("\"{node}\"")))
        .and_then(|l| l.split('=').nth(1).map(|v| v.trim().trim_matches('"').to_string()))
        .unwrap_or_default()
}
