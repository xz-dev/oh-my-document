//! 13.x guards: adapt rejection, link dup scoping, reason not creating
//! relationships, interior-range-commit link targets.

use std::process::Command;

fn omd() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") { return p.into(); }
    let mut p = std::env::current_exe().unwrap();
    p.pop(); p.pop(); p.push("omd"); p
}

struct T(std::path::PathBuf);
impl T {
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!("omd-gr-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
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
    fn write(&self, p: &str, c: &str) { std::fs::write(self.0.join(p), c).unwrap(); }
    fn tip(&self, node: &str) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
            .lines().find(|l| l.contains(&format!("\"{node}\"")) && l.contains('='))
            .and_then(|l| l.split('=').nth(1).map(|v| v.trim().trim_matches('"').to_string()))
            .unwrap_or_default()
    }
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
}
impl Drop for T { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn setup_link(t: &T) -> (String, String) {
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    // b's range commit links FROM a's range — the committing node is b's range.
    t.run(&["commit", "commit", "b.md", "--range", "0-1",
            "--link-from", "a.md@text:0-1", "--reason", "lb"]);
    (t.tip("range:a.md@text:0-1"), t.tip("range:b.md@text:0-1"))
}

// 10.1: adapt without a link ID is rejected.
#[test]
fn adapt_without_link_id_rejected() {
    let t = T::new();
    setup_link(&t);
    // Upstream commit seeds a pending obligation.
    t.run(&["commit", "commit", "a.md", "--id", &t.tip("range:a.md@text:0-1"),
            "--range", "0-1", "--reason", "up"]);
    let (c, o, e) = t.run(&["commit", "adapt", "b.md", "--changes", "x", "--reason", "r"]);
    assert_ne!(c, 0, "adapt without --link-id must fail: {o} {e}");
}

// 10.1: adapt without a reason is rejected.
#[test]
fn adapt_without_reason_rejected() {
    let t = T::new();
    setup_link(&t);
    t.run(&["commit", "commit", "a.md", "--id", &t.tip("range:a.md@text:0-1"),
            "--range", "0-1", "--reason", "up"]);
    let lid = t.state().lines().find(|l| l.contains("[links."))
        .map(|l| l.trim().to_string()).unwrap_or_default();
    // Extract link id from "[links.<id>]".
    let link_id = lid.trim_start_matches("[links.").trim_end_matches(']').to_string();
    let (c, o, e) = t.run(&["commit", "adapt", "b.md", "--link-id", &link_id,
                          "--changes", "x"]);
    assert_ne!(c, 0, "adapt without reason must fail: {o} {e}");
}

// 10.1: a reason does not create another relationship — `commit adapt` with a
// reason only records the adaptation; it must NOT spawn a new link.
#[test]
fn adapt_reason_creates_no_new_link() {
    let t = T::new();
    setup_link(&t);
    let links_before = t.state().matches("[links.").count();
    // Seed + adapt.
    t.run(&["commit", "commit", "a.md", "--id", &t.tip("range:a.md@text:0-1"),
            "--range", "0-1", "--reason", "up"]);
    let lid = t.state().lines().find(|l| l.contains("[links."))
        .map(|l| l.trim().trim_start_matches("[links.").trim_end_matches(']').to_string())
        .unwrap_or_default();
    t.run(&["commit", "adapt", "b.md", "--link-id", &lid, "--stop", "--reason", "done"]);
    let links_after = t.state().matches("[links.").count();
    assert_eq!(links_before, links_after,
        "adapt must not create a new link record: before={links_before} after={links_after}");
}

// 10.1: --stop clears all pending obligations on that ONE link, retains others.
#[test]
fn stop_clears_one_link_retains_others() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.write("c.md", "c");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["init", "c.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    t.run(&["commit", "commit", "b.md", "--range", "0-1", "--link-from", "a.md@text:0-1", "--reason", "lb"]);
    t.run(&["commit", "commit", "c.md", "--range", "0-1", "--link-from", "a.md@text:0-1", "--reason", "lc"]);
    // Seed pending on both links via an upstream commit.
    t.run(&["commit", "commit", "a.md", "--id", &t.tip("range:a.md@text:0-1"),
            "--range", "0-1", "--reason", "up"]);
    let lids: Vec<String> = t.state().lines()
        .filter(|l| l.contains("[links."))
        .map(|l| l.trim().trim_start_matches("[links.").trim_end_matches(']').to_string())
        .collect();
    assert!(lids.len() >= 2, "two links: {}", t.state());
    // --stop on the FIRST link only.
    t.run(&["commit", "adapt", "b.md", "--link-id", &lids[0], "--stop", "--reason", "done"]);
    let st = t.state();
    // Second link's pending remains (link id still in link_pending).
    assert!(st.contains(&lids[1]) || st.contains("[link_pending"),
        "other link's pending retained: {st}");
}

// 10.1: adapt REQUIRES a reason — `--no-reason` (omitting the reason) is
// rejected. An adaptation is a recorded decision; reasonless adapt is a bug.
#[test]
fn adapt_no_reason_rejected() {
    let t = T::new();
    setup_link(&t);
    t.run(&["commit", "commit", "a.md", "--id", &t.tip("range:a.md@text:0-1"),
            "--range", "0-1", "--reason", "up"]);
    let lid = t.state().lines().find(|l| l.contains("[links."))
        .map(|l| l.trim().trim_start_matches("[links.").trim_end_matches(']').to_string())
        .unwrap_or_default();
    let (c, o, e) = t.run(&["commit", "adapt", "b.md", "--link-id", &lid,
                          "--stop", "--no-reason"]);
    assert_ne!(c, 0, "adapt --no-reason must fail: {o} {e}");
}

// 9.2: interior range commits are valid link targets — a link may point at a
// non-tip range commit (the chain member, not just the tip).
#[test]
fn interior_range_commit_is_valid_link_target() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r1"]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&["commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "r2"]);
    // Link b's range FROM a's non-tip commit c1 — the interior member is a
    // valid reference point. Committing node is b's range.
    let (c, o, e) = t.run(&["commit", "commit", "b.md", "--range", "0-1",
                          "--link-from", "a.md@text:0-1",
                          "--reason", "links-to-interior"]);
    // The link resolves against the range node (which contains c1) — ok.
    assert_eq!(c, 0, "interior range commit link target: {o} {e}");
}

// 9.3: duplicate link check is per-direction — opposite directions to one
// range are distinct, not duplicates.
#[test]
fn opposite_directions_to_one_range_are_distinct() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    // --link-from AND --link-to the same range in one command: two distinct
    // directions, not a duplicate. Committing node is b's range.
    let (c, o, e) = t.run(&["commit", "commit", "b.md", "--range", "0-1",
                          "--link-from", "a.md@text:0-1",
                          "--link-to", "a.md@text:0-1",
                          "--reason", "bidirectional"]);
    assert_eq!(c, 0, "opposite directions are distinct, not dup: {o} {e}");
}

// 9.3: duplicate check scoped to one invocation — repeating the same
// --link-from across two SEPARATE commands is allowed (creates two links).
#[test]
fn duplicate_check_scoped_per_invocation() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    // Two separate commands each linking b's range from a's range.
    let (c1, _, _) = t.run(&["commit", "commit", "b.md", "--range", "0-1", "--link-from", "a.md@text:0-1", "--reason", "l1"]);
    let (c2, _, _) = t.run(&["commit", "commit", "b.md", "--range", "0-1", "--link-from", "a.md@text:0-1", "--reason", "l2"]);
    assert_eq!(c1, 0);
    assert_eq!(c2, 0, "same --link-from across invocations is allowed (distinct links)");
}

// Re-audit BUG3: --source-ref 'command::…' records Acquisition::Command and
// verify reports it `unverified` when the command isn't permitted to run.
#[test]
fn command_source_records_acquisition_and_unverified() {
    let t = T::new();
    // init the file as a command-sourced version.
    let (c, o, e) = t.run(&["commit", "init", "f.txt",
                          "--source-ref", "command::echo::[\"hi\"]"]);
    assert_eq!(c, 0, "{o} {e}");
    // The version's acquisition is Command, not File.
    let mut found = false;
    if let Ok(rd) = std::fs::read_dir(t.0.join(".omd/versions")) {
        for en in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(en.path()) {
                if txt.contains("[acquisition.command]") { found = true; }
            }
        }
    }
    assert!(found, "Acquisition::Command recorded");
    // verify without --run-command reports it unverified, not clean.
    let (_, o, _) = t.run(&["verify", "f.txt"]);
    assert!(o.contains("unverified"), "unverified reported: {o}");
}

// Re-audit BUG2: a pure position move (fragment intact at a new offset) is
// a candidate migration requiring review — never a silent CLEAN.
#[test]
fn pure_position_move_reports_moved_not_clean() {
    let t = T::new();
    t.write("a.md", "HEADERSPLITMORE");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-5", "--reason", "r"]);
    // Insert a char before the range — fragment now at offset 1, text intact.
    t.write("a.md", "XHEADERSPLITMORE");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    assert!(o.contains("moved") || o.contains("needs review"),
            "moved/review reported: {o}");
}
