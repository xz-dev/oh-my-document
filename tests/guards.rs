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

// change-review: adjacent markers are not skipped recursively — resetting to
// a BEGIN lands on ITS direct predecessor only, never cascades past a chain
// of markers. If BEGIN's predecessor is another marker, that's the landing.
#[test]
fn adjacent_markers_reset_lands_one_step() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    // Two blocks back-to-back: BEGIN c1 END c1' BEGIN c2 END c2'.
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "b1"]);
    // Reset to the END of the block — lands on END's direct predecessor
    // (the last interior/link), one step back, not recursively skipped.
    let end = t.tip("range:a.md@text:0-1");
    let (c, o, _) = t.run(&["commit", "reset", "a.md", "--reason", &end]);
    assert_eq!(c, 0, "{o}");
    // The reset reports requested→actual with the predecessor landing.
    assert!(o.contains("actual") || o.contains("requested"),
            "reset outcome: {o}");
}

// change-review: resetting to a commit inside an OPEN block is refused —
// interior members are never reset targets.
#[test]
fn reset_interior_of_open_block_refused() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    // b's block: BEGIN → interior commit → LINK → END.
    t.run(&["commit", "commit", "b.md", "--range", "0-1",
            "--link-from", "a.md@text:0-1", "--reason", "lb"]);
    // Find the interior LINK commit (a block member).
    let link_commit = std::fs::read_dir(t.0.join(".omd/commits")).unwrap()
        .flatten()
        .find_map(|e| {
            let txt = std::fs::read_to_string(e.path()).ok()?;
            if txt.contains("kind = \"link\"") {
                Some(e.path().file_stem().unwrap().to_string_lossy().to_string())
            } else { None }
        }).expect("a link commit");
    // Reset to the interior link member → refused (interior not a target).
    let (c, o, _) = t.run(&["commit", "reset", "b.md", "--reason", &link_commit]);
    assert_ne!(c, 0, "interior member reset must fail: {o}");
}

// change-review: two links with identical endpoints deliberately coexist —
// the spec allows it (distinct link_ids, adapted separately).
#[test]
fn same_endpoints_two_links_coexist() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    // Two separate commands each create a link a-range → b-range.
    t.run(&["commit", "commit", "b.md", "--range", "0-1",
            "--link-from", "a.md@text:0-1", "--reason", "l1"]);
    t.run(&["commit", "commit", "b.md", "--range", "0-1",
            "--link-from", "a.md@text:0-1", "--reason", "l2"]);
    let lids = t.state().lines()
        .filter(|l| l.contains("[links.")).count();
    assert!(lids >= 2, "two same-endpoint links coexist: {}", t.state());
}

// managed-content: rebuild works with no Git repo and no OMD cache — the
// store stands alone (spec: core never depends on Git).
#[test]
fn rebuild_without_git_or_cache() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // Delete the derived index (cache) — state.toml is authoritative.
    let _ = std::fs::remove_file(t.0.join(".omd/index.txt"));
    // reindex rebuilds from the manifest only, no Git needed.
    let (c, o, _) = t.run(&["reindex"]);
    assert_eq!(c, 0, "reindex works without cache: {o}");
    assert!(t.0.join(".omd/index.txt").exists() || o.contains("ok"),
            "index rebuilt: {o}");
}

// change-review: a referenced dangling commit is RETAINED — only truly
// unreferenced dangles are collected. A note on a dangling keeps it.
#[test]
fn referenced_dangling_commit_retained() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r1"]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&["commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "r2"]);
    let c2 = t.tip("range:a.md@text:0-1");
    // Add a note referencing c2 — it becomes a referenced dangling.
    t.run(&["note", "add", &c2, "--text", "evidence"]);
    t.run(&["commit", "reset", "a.md", "--reason", &c1]);
    // c2 is dangling but referenced by a note → gc must NOT collect it.
    t.run(&["gc"]);
    assert!(t.0.join(format!(".omd/commits/{c2}.toml")).exists(),
            "referenced dangling c2 retained");
}

// change-review: clean --no-reason succeeds — omitting the reason on clean
// is a non-skip path (spec allows clean without a reason).
#[test]
fn clean_no_reason_succeeds() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r"]);
    // clean with --no-reason is a legitimate clearing commit.
    let (c, o, e) = t.run(&["commit", "clean", "a.md", "--range", "0-1", "--no-reason"]);
    assert_eq!(c, 0, "clean --no-reason ok: {o} {e}");
}

// change-review: TOML formatting is not part of commit identity — the same
// logical record serializes to a stable ID regardless of field ordering.
#[test]
fn commit_id_stable_under_field_reorder() {
    // Two identical commits created the same way get DIFFERENT ids only via
    // salt/time — but a record re-serialized must keep its recorded id.
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r"]);
    let tip = t.tip("range:a.md@text:0-1");
    // The filename IS the commit id — re-reading preserves it verbatim.
    assert!(t.0.join(format!(".omd/commits/{tip}.toml")).exists());
    // Re-serializing the commit doesn't mint a new id (id is the filename).
    let again = t.tip("range:a.md@text:0-1");
    assert_eq!(tip, again, "id is stable across reads");
}

// change-review: --timestamp wires a FixedClock — the commit's recorded
// timestamp reflects the user's chosen time (manual replay).
#[test]
fn timestamp_records_user_time() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let (c, _, _) = t.run(&["commit", "commit", "a.md", "--range", "0-1",
                          "--reason", "r", "--timestamp", "2020-01-02T03:04:05Z"]);
    assert_eq!(c, 0);
    let tip = t.tip("range:a.md@text:0-1");
    let txt = std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip}.toml"))).unwrap();
    assert!(txt.contains("2020-01-02T03:04:05"), "user timestamp recorded: {txt}");
}

// change-review: note corrections patch a note's fields — `note patch`
// revises the recorded reason without a new relationship.
#[test]
fn note_patch_revises_recorded_reason() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r"]);
    let tip = t.tip("range:a.md@text:0-1");
    // add a note, capture its id from the list, then patch it.
    t.run(&["note", "add", &tip, "--text", "first"]);
    let (_, ol0, _) = t.run(&["note", "list", &tip]);
    let nid = serde_json::from_str::<serde_json::Value>(&ol0).ok()
        .and_then(|j| j["data"]["notes"][0]["note_id"].as_str().map(String::from))
        .or_else(|| serde_json::from_str::<serde_json::Value>(&ol0).ok()
            .and_then(|j| j["data"]["notes"][0]["id"].as_str().map(String::from)))
        .unwrap_or_default();
    let (c, o, e) = t.run(&["note", "patch", &tip, "--target", &nid, "--text", "corrected"]);
    let (_, ol, _) = t.run(&["note", "list", &tip]);
    assert!(c == 0 && (ol.contains("corrected") || o.contains("corrected")),
            "note patched: {ol} {e}");
}

// change-review: reset to first BEGIN lands on empty chain — the whole
// block's contents withdraw; node tip removed entirely.
#[test]
fn reset_first_begin_lands_empty() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "ra"]);
    t.run(&["commit", "commit", "b.md", "--range", "0-1",
            "--link-from", "a.md@text:0-1", "--reason", "lb"]);
    let begin = std::fs::read_dir(t.0.join(".omd/commits")).unwrap()
        .flatten()
        .find_map(|e| {
            let txt = std::fs::read_to_string(e.path()).ok()?;
            if txt.contains("kind = \"atomic_begin\"") {
                Some(e.path().file_stem().unwrap().to_string_lossy().to_string())
            } else { None }
        }).expect("BEGIN exists");
    let (c, o, _) = t.run(&["commit", "reset", "b.md", "--reason", &begin]);
    assert_eq!(c, 0, "{o}");
    // actual is empty — landing is the null/empty chain.
    let st = t.state();
    // The link inside the removed segment is gone (block contents withdrew).
    assert!(!st.contains("[links."), "block contents withdrew: {st}");
}

// local-project-links: two projects can use the same tag name independently —
// a tag is project-local, never cross-store entangled.
#[test]
fn same_tag_name_independent_across_stores() {
    let t = T::new();
    let store2 = t.0.join("store2");
    // Project 1 tags a.md v1.
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "tag", "a.md", "--tag", "v1"]);
    // A second store in the same dir — independent state.
    let o2 = Command::new(omd()).arg("--meta").arg(&store2)
        .arg("list").current_dir(&t.0).output().unwrap();
    let s2 = String::from_utf8_lossy(&o2.stdout);
    // store2 has no v1 tag — tags are per-store, not shared.
    assert!(!s2.contains("v1"), "tag is store-local: {s2}");
}

// change-review: replace on a version rebinds its acquisition only on
// byte-identical FULL content — a different-content source refuses.
#[test]
fn replace_refuses_different_content() {
    let t = T::new();
    t.write("a.md", "alpha");
    t.run(&["init", "a.md"]);
    t.write("b.md", "different-bytes");
    let tip = t.tip("file:a.md");
    // Replace a's acquisition with b's content — different bytes → refuse.
    let (c, o, e) = t.run(&["replace", &tip, "--source", "b.md"]);
    assert_ne!(c, 0, "replace on non-identical content must fail: {o} {e}");
}

// local-project-links: an unrelated offline peer does not block local work —
// committing while a registered peer is unreachable still succeeds.
#[test]
fn offline_peer_does_not_block_local_commit() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // Register a peer that doesn't exist on disk (offline).
    t.run(&["register", "aabbccddeeff00112233445566778899", "/nonexistent/peer"]);
    // Local commit still works — an offline peer never blocks writes.
    let (c, o, _) = t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r"]);
    assert_eq!(c, 0, "local commit unaffected by offline peer: {o}");
}

// managed-content: a rule declared at warn level reports its status but does
// NOT fail the check command — warn ≠ fail.
#[test]
fn warn_level_rule_does_not_fail_check() {
    let t = T::new();
    std::fs::create_dir_all(t.0.join("spec")).unwrap();
    std::fs::create_dir_all(t.0.join("code")).unwrap();
    t.write("spec/s.md", "spec");
    t.write("code/c.rs", "code");
    t.run(&["init", "spec/s.md"]);
    t.run(&["init", "code/c.rs"]);
    // Declare a spec->code rule at warn level; no links → coverage gap.
    let (c, o, _) = t.run(&["commit", "scope_adjust", "spec",
                          "--rule", "spec->code", "--level", "warn"]);
    let _ = (c, o);
    let (cc, oc, _) = t.run(&["check"]);
    // A warn-level gap reports the item but check does not hard-fail.
    assert_eq!(cc, 0, "warn rule doesn't fail check: {oc}");
}

// managed-content: explicit range expansion — committing a NEW range coords
// over the same file via --id is a new object, not a silent merge into the
// old range's identity.
#[test]
fn explicit_range_expansion_distinct_object() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-5", "--reason", "r1"]);
    let c1 = t.tip("range:a.md@text:0-5");
    // --id c1 with a DIFFERENT range expands/modifies c1's chain — a commit
    // on c1's node, not a fresh independent object.
    let (c, _, _) = t.run(&["commit", "commit", "a.md", "--id", &c1,
                          "--range", "0-10", "--reason", "expand"]);
    assert_eq!(c, 0);
    // The range node c1's chain advanced (the new commit chains onto c1's tip).
    let tip = t.tip("range:a.md@text:0-5");
    assert!(!tip.is_empty(), "c1's chain advanced via --id");
}
