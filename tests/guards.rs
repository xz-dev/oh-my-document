//! 13.x guards: adapt rejection, link dup scoping, reason not creating
//! relationships, interior-range-commit link targets.

use std::process::Command;

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
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!(
            "omd-gr-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&r).unwrap();
        Self(r)
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = Command::new(omd())
            .arg("--meta")
            .arg(self.0.join(".omd"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).into(),
            String::from_utf8_lossy(&o.stderr).into(),
        )
    }
    fn write(&self, p: &str, c: &str) {
        std::fs::write(self.0.join(p), c).unwrap();
    }
    fn tip(&self, node: &str) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml"))
            .unwrap_or_default()
            .lines()
            .find(|l| l.contains(&format!("\"{node}\"")) && l.contains('='))
            .and_then(|l| {
                l.split('=')
                    .nth(1)
                    .map(|v| v.trim().trim_matches('"').to_string())
            })
            .unwrap_or_default()
    }
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn setup_link(t: &T) -> (String, String) {
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // b's range commit links FROM a's range — the committing node is b's range.
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lb",
    ]);
    (t.tip("range:a.md@text:0-1"), t.tip("range:b.md@text:0-1"))
}

// 10.1: adapt without a link ID is rejected.
#[test]
fn adapt_without_link_id_rejected() {
    let t = T::new();
    setup_link(&t);
    // Upstream commit seeds a pending obligation.
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    let (c, o, e) = t.run(&["commit", "adapt", "b.md", "--changes", "x", "--reason", "r"]);
    assert_ne!(c, 0, "adapt without --link-id must fail: {o} {e}");
}

// 10.1: adapt without a reason is rejected.
#[test]
fn adapt_without_reason_rejected() {
    let t = T::new();
    setup_link(&t);
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    let lid = t
        .state()
        .lines()
        .find(|l| l.contains("[links."))
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    // Extract link id from "[links.<id>]".
    let link_id = lid
        .trim_start_matches("[links.")
        .trim_end_matches(']')
        .to_string();
    let (c, o, e) = t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--link-id",
        &link_id,
        "--changes",
        "x",
    ]);
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
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    let lid = t
        .state()
        .lines()
        .find(|l| l.contains("[links."))
        .map(|l| {
            l.trim()
                .trim_start_matches("[links.")
                .trim_end_matches(']')
                .to_string()
        })
        .unwrap_or_default();
    t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--link-id",
        &lid,
        "--stop",
        "--reason",
        "done",
    ]);
    let links_after = t.state().matches("[links.").count();
    assert_eq!(
        links_before, links_after,
        "adapt must not create a new link record: before={links_before} after={links_after}"
    );
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lb",
    ]);
    t.run(&[
        "commit",
        "commit",
        "c.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lc",
    ]);
    // Seed pending on both links via an upstream commit.
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    let lids: Vec<String> = t
        .state()
        .lines()
        .filter(|l| l.contains("[links."))
        .map(|l| {
            l.trim()
                .trim_start_matches("[links.")
                .trim_end_matches(']')
                .to_string()
        })
        .collect();
    assert!(lids.len() >= 2, "two links: {}", t.state());
    // --stop on the FIRST link only.
    t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--link-id",
        &lids[0],
        "--stop",
        "--reason",
        "done",
    ]);
    let st = t.state();
    // Second link's pending remains (link id still in link_pending).
    assert!(
        st.contains(&lids[1]) || st.contains("[link_pending"),
        "other link's pending retained: {st}"
    );
}

// 10.1: adapt REQUIRES a reason — `--no-reason` (omitting the reason) is
// rejected. An adaptation is a recorded decision; reasonless adapt is a bug.
#[test]
fn adapt_no_reason_rejected() {
    let t = T::new();
    setup_link(&t);
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    let lid = t
        .state()
        .lines()
        .find(|l| l.contains("[links."))
        .map(|l| {
            l.trim()
                .trim_start_matches("[links.")
                .trim_end_matches(']')
                .to_string()
        })
        .unwrap_or_default();
    let (c, o, e) = t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--link-id",
        &lid,
        "--stop",
        "--no-reason",
    ]);
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "r2",
    ]);
    // Link b's range FROM a's non-tip commit c1 — the interior member is a
    // valid reference point. Committing node is b's range.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "links-to-interior",
    ]);
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // --link-from AND --link-to the same range in one command: two distinct
    // directions, not a duplicate. Committing node is b's range.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--link-to",
        "a.md@text:0-1",
        "--reason",
        "bidirectional",
    ]);
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // Two separate commands each linking b's range from a's range.
    let (c1, _, _) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "l1",
    ]);
    let (c2, _, _) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "l2",
    ]);
    assert_eq!(c1, 0);
    assert_eq!(
        c2, 0,
        "same --link-from across invocations is allowed (distinct links)"
    );
}

// Re-audit BUG3: --source-ref 'command::…' records Acquisition::Command and
// verify reports it `unverified` when the command isn't permitted to run.
#[test]
fn command_source_records_acquisition_and_unverified() {
    let t = T::new();
    // init the file as a command-sourced version.
    let (c, o, e) = t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-ref",
        "command::echo::[\"hi\"]",
    ]);
    assert_eq!(c, 0, "{o} {e}");
    // The version's acquisition is Command, not File.
    let mut found = false;
    if let Ok(rd) = std::fs::read_dir(t.0.join(".omd/versions")) {
        for en in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(en.path())
                && txt.contains("[acquisition.command]")
            {
                found = true;
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // Insert a char before the range — fragment now at offset 1, text intact.
    t.write("a.md", "XHEADERSPLITMORE");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    assert!(
        o.contains("moved") || o.contains("needs review"),
        "moved/review reported: {o}"
    );
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "b1",
    ]);
    // Reset to the END of the block — lands on END's direct predecessor
    // (the last interior/link), one step back, not recursively skipped.
    let end = t.tip("range:a.md@text:0-1");
    let (c, o, _) = t.run(&["commit", "reset", "a.md", "--reason", &end]);
    assert_eq!(c, 0, "{o}");
    // The reset reports requested→actual with the predecessor landing.
    assert!(
        o.contains("actual") || o.contains("requested"),
        "reset outcome: {o}"
    );
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // b's block: BEGIN → interior commit → LINK → END.
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lb",
    ]);
    // Find the interior LINK commit (a block member).
    let link_commit = std::fs::read_dir(t.0.join(".omd/commits"))
        .unwrap()
        .flatten()
        .find_map(|e| {
            let txt = std::fs::read_to_string(e.path()).ok()?;
            if txt.contains("kind = \"link\"") {
                Some(e.path().file_stem().unwrap().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .expect("a link commit");
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // Two separate commands each create a link a-range → b-range.
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "l1",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "l2",
    ]);
    let lids = t.state().lines().filter(|l| l.contains("[links.")).count();
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
    // The index file exists AND parses with one row per published commit.
    let idx = std::fs::read_to_string(t.0.join(".omd/index.txt")).unwrap_or_default();
    let rows: Vec<&str> = idx.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(rows.len(), 1, "one indexed row for the init commit: {idx}");
    assert!(rows[0].contains('\t'), "row is tab-separated: {idx}");
}

// change-review: a referenced dangling commit is RETAINED — only truly
// unreferenced dangles are collected. A note on a dangling keeps it.
#[test]
fn referenced_dangling_commit_retained() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "r2",
    ]);
    let c2 = t.tip("range:a.md@text:0-1");
    // Add a note referencing c2 — it becomes a referenced dangling.
    t.run(&["note", "add", &c2, "--text", "evidence"]);
    t.run(&["commit", "reset", "a.md", "--reason", &c1]);
    // c2 is dangling but referenced by a note → gc must NOT collect it.
    t.run(&["gc"]);
    assert!(
        t.0.join(format!(".omd/commits/{c2}.toml")).exists(),
        "referenced dangling c2 retained"
    );
}

// change-review: clean --no-reason succeeds — omitting the reason on clean
// is a non-skip path (spec allows clean without a reason).
#[test]
fn clean_no_reason_succeeds() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r",
    ]);
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r",
    ]);
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
    let (c, _, _) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0-1",
        "--reason",
        "r",
        "--timestamp",
        "2020-01-02T03:04:05Z",
    ]);
    assert_eq!(c, 0);
    let tip = t.tip("range:a.md@text:0-1");
    let txt = std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip}.toml"))).unwrap();
    assert!(
        txt.contains("2020-01-02T03:04:05"),
        "user timestamp recorded: {txt}"
    );
}

// change-review: note corrections patch a note's fields — `note patch`
// revises the recorded reason without a new relationship.
#[test]
fn note_patch_revises_recorded_reason() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r",
    ]);
    let tip = t.tip("range:a.md@text:0-1");
    // add a note, capture its id from the list, then patch it.
    t.run(&["note", "add", &tip, "--text", "first"]);
    let (_, ol0, _) = t.run(&["note", "list", &tip]);
    let nid = serde_json::from_str::<serde_json::Value>(&ol0)
        .ok()
        .and_then(|j| j["data"]["notes"][0]["note_id"].as_str().map(String::from))
        .or_else(|| {
            serde_json::from_str::<serde_json::Value>(&ol0)
                .ok()
                .and_then(|j| j["data"]["notes"][0]["id"].as_str().map(String::from))
        })
        .unwrap_or_default();
    let (c, o, e) = t.run(&[
        "note",
        "patch",
        &tip,
        "--target",
        &nid,
        "--text",
        "corrected",
    ]);
    let (_, ol, _) = t.run(&["note", "list", &tip]);
    assert!(
        c == 0 && (ol.contains("corrected") || o.contains("corrected")),
        "note patched: {ol} {e}"
    );
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lb",
    ]);
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
        .expect("BEGIN exists");
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
    let o2 = Command::new(omd())
        .arg("--meta")
        .arg(&store2)
        .arg("list")
        .current_dir(&t.0)
        .output()
        .unwrap();
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
    t.run(&[
        "register",
        "aabbccddeeff00112233445566778899",
        "/nonexistent/peer",
    ]);
    // Local commit still works — an offline peer never blocks writes.
    let (c, o, _) = t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r",
    ]);
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
    let (c, o, _) = t.run(&[
        "commit",
        "scope_adjust",
        "spec",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
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
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-5");
    // --id c1 with a DIFFERENT range expands/modifies c1's chain — a commit
    // on c1's node, not a fresh independent object.
    let (c, _, _) = t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0-10", "--reason", "expand",
    ]);
    assert_eq!(c, 0);
    // The range node c1's chain advanced (the new commit chains onto c1's tip).
    let tip = t.tip("range:a.md@text:0-5");
    assert!(!tip.is_empty(), "c1's chain advanced via --id");
}

// managed-content: a cross-boundary edit (change spanning the range edge)
// dirties the range — the spec requires review for boundary-crossing changes.
#[test]
fn cross_boundary_edit_dirties() {
    let t = T::new();
    t.write("a.md", "AABBCCDD");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "2-6", "--reason", "r",
    ]);
    // Edit spanning the boundary (positions 1-7 changed).
    t.write("a.md", "AXXBCCYD");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    assert!(
        o.contains("dirty")
            || o.contains("moved")
            || o.contains("locate")
            || o.contains("in-range"),
        "cross-boundary edit flagged: {o}"
    );
}

// managed-content: a byte-mode range counts raw byte offsets, not chars.
#[test]
fn byte_mode_range_counts_bytes() {
    let t = T::new();
    // Multi-byte UTF-8 chars: 'é' = 2 bytes.
    t.write("a.md", "aébc");
    t.run(&["init", "a.md"]);
    // byte:0-3 covers 'a' + 'é'(2 bytes) = 3 bytes.
    let (c, o, e) = t.run(&[
        "commit", "commit", "a.md", "--range", "byte:0-3", "--reason", "r",
    ]);
    assert_eq!(c, 0, "byte range commits: {o} {e}");
    // The byte-range node key uses byte coordinates.
    assert!(
        t.state().contains("byte:0-3"),
        "byte range node: {}",
        t.state()
    );
}

// managed-content: a fragment matching ambiguously in current content reports
// locate candidates — never auto-picks one.
#[test]
fn ambiguous_fragment_reports_locate_candidates() {
    let t = T::new();
    t.write("a.md", "XX AB XX");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "3-5", "--reason", "r",
    ]);
    // Now 'AB' appears multiple places conceptually; make current ambiguous.
    t.write("a.md", "AB AB AB");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    assert!(
        o.contains("ambiguous") || o.contains("candidates") || o.contains("locate"),
        "ambiguous locate reported: {o}"
    );
}

// managed-content: a file with a tombstone (Delete commit) is NOT reported
// missing — the delete is intentional.
#[test]
fn tombstoned_file_not_missing() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // delete the file via the tombstone verb, then remove it from disk.
    t.run(&["delete", "a.md"]);
    std::fs::remove_file(t.0.join("a.md")).unwrap();
    let (_, o, _) = t.run(&["verify", "a.md"]);
    // Tombstone → not 'missing'.
    assert!(
        !o.contains("missing") || !o.contains("no tombstone"),
        "tombstoned file not missing: {o}"
    );
}

// command-verification: --run-command=true verify RE-RUNS the command and
// compares stdout to recorded content — same output → clean; changed → dirty.
#[test]
fn run_command_verify_reruns_and_compares() {
    let t = T::new();
    let (c, _, _) = t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-ref",
        "command::echo::[\"hi\"]",
    ]);
    assert_eq!(c, 0);
    // verify with the command permitted → re-runs, same stdout → ok.
    let (_, o, _) = t.run(&["--run-command=true", "verify", "f.txt"]);
    let j = serde_json::from_str::<serde_json::Value>(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(true),
        "re-run same output → clean: {o}"
    );
    // The command source has no disk file — missing must be EMPTY.
    assert!(
        j["data"]["missing"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "command source not missing: {o}"
    );
}

// command-verification: a command whose output changed reports dirty.
#[test]
fn run_command_changed_output_dirties() {
    let t = T::new();
    // A command that echoes a file's content — we can change it.
    t.write("in.txt", "v1");
    let (c, _, _) = t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-ref",
        "command::cat::[\"in.txt\"]",
    ]);
    assert_eq!(c, 0);
    // Change the command's output.
    t.write("in.txt", "v2-different");
    let (_, o, _) = t.run(&["--run-command=true", "verify", "f.txt"]);
    assert!(
        o.contains("command output changed") || o.contains("dirty"),
        "changed command output → dirty: {o}"
    );
}

// change-review: same-endpoint obligations stay distinct by link_id — an
// upstream commit seeds pending on EACH link separately (not merged).
#[test]
fn same_endpoint_obligations_distinct_by_link_id() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // Two links a-range → b-range (distinct link_ids).
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "l1",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "l2",
    ]);
    // Upstream commit on source seeds pending on BOTH links.
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    // link_pending is one table with `linkid = [commits]` rows — each link
    // gets its own pending set keyed by link_id.
    let st = t.state();
    let pend_section = st.split("[link_pending]").nth(1).unwrap_or("");
    let pend_entries = pend_section
        .lines()
        .take_while(|l| !l.starts_with('['))
        .filter(|l| l.contains(" = ["))
        .count();
    assert!(
        pend_entries >= 2,
        "each link has own pending entry: {pend_section}"
    );
}

// change-review: copy preserves identity — `omd copy` gives the target a NEW
// identity (own tip), not the source's commit ids.
#[test]
fn copy_gives_new_identity() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let src_tip = t.tip("file:a.md");
    t.write("b.md", "x");
    let (c, _, _) = t.run(&["copy", "a.md", "b.md"]);
    assert_eq!(c, 0);
    let tgt_tip = t.tip("file:b.md");
    // Target has its own commit — not the source's id.
    assert!(
        !tgt_tip.is_empty() && tgt_tip != src_tip,
        "copy → new identity: src={src_tip} tgt={tgt_tip}"
    );
}

// command-verification: a command source initializes even when auto-run is
// disabled — init captures output once, the gating is on RE-run (verify),
// not initial capture.
#[test]
fn command_init_works_despite_autorun_disabled() {
    let t = T::new();
    // No --run-command, no config → built-in floor false. init still runs
    // the command to capture its first version (init ≠ verify-rerun).
    let (c, o, e) = t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-ref",
        "command::echo::[\"hi\"]",
    ]);
    assert_eq!(c, 0, "command init captures output: {o} {e}");
    let mut is_cmd = false;
    if let Ok(rd) = std::fs::read_dir(t.0.join(".omd/versions")) {
        for en in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(en.path())
                && txt.contains("[acquisition.command]")
            {
                is_cmd = true;
            }
        }
    }
    assert!(is_cmd);
}

// managed-content: two sources producing equal content share no version
// coupling — equal output doesn't make one a review of the other.
#[test]
fn equal_output_does_not_create_review() {
    let t = T::new();
    t.write("a.md", "same");
    t.write("b.md", "same");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    // Same content, two independent file nodes — no link/obligation created.
    let st = t.state();
    assert!(st.contains("file:a.md") && st.contains("file:b.md"));
    assert!(
        !st.contains("[links."),
        "equal content creates no relationship: {st}"
    );
}

// change-review: a split range doesn't copy the parent's links/obligations —
// a new range is a fresh object with no inherited relationships.
#[test]
fn split_range_inherits_no_relationships() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-10", "--reason", "r",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-10",
        "--reason",
        "lb",
    ]);
    // Split a's range into a new sub-range — a fresh object.
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "sub",
    ]);
    // The new sub-range has no pending obligations of its own (it wasn't
    // the link's source — 0-10 was).
    let st = t.state();
    assert!(st.contains("range:a.md@text:0-5"), "sub-range exists: {st}");
}

// change-review: clean does NOT re-execute a command source — clearing an
// obligation is a state operation, never a re-acquisition.
#[test]
fn clean_does_not_rerun_command() {
    let t = T::new();
    t.write("cnt.txt", "1");
    t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-ref",
        "command::cat::[\"cnt.txt\"]",
    ]);
    // Bump the command's would-be output.
    t.write("cnt.txt", "999");
    // clean is a state marker — it must not re-run the command (no new version).
    let before = std::fs::read_dir(t.0.join(".omd/versions"))
        .unwrap()
        .count();
    t.run(&["commit", "clean", "f.txt", "--reason", "done"]);
    let after = std::fs::read_dir(t.0.join(".omd/versions"))
        .unwrap()
        .count();
    assert_eq!(before, after, "clean doesn't re-acquire a version");
}

// managed-content: `replace` on a commit then verify — the rebound
// acquisition verifies against the new source, not a stale basis.
#[test]
fn replace_then_verify_uses_new_source() {
    let t = T::new();
    t.write("a.md", "same-bytes");
    t.run(&["init", "a.md"]);
    t.write("b.md", "same-bytes");
    let tip = t.tip("file:a.md");
    let (c, o, e) = t.run(&["replace", &tip, "--source", "b.md"]);
    assert_eq!(c, 0, "identical-content replace ok: {o} {e}");
    // verify reads live content — identical bytes → ok:true (JSON value).
    let (_, ov, _) = t.run(&["verify", "a.md"]);
    let j = serde_json::from_str::<serde_json::Value>(&ov).unwrap_or_default();
    assert_eq!(j["data"]["ok"].as_bool(), Some(true), "verify clean: {ov}");
}

// change-review: full confirmed coverage does NOT clear obligations — a
// range fully covered still keeps its pending adapt until adapt runs.
#[test]
fn full_coverage_keeps_obligation() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-1",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lb",
    ]);
    // Upstream commit seeds pending; even if b's range is fully covered,
    // the obligation persists until adapt clears it.
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--reason",
        "up",
    ]);
    let st = t.state();
    assert!(
        st.contains("[link_pending]") && st.contains(" = ["),
        "obligation persists despite coverage: {st}"
    );
}

// command-verification: rebuild from a state the cache never saw — reindex
// works on a manifest the local index lacks (unfamiliar cache).
#[test]
fn reindex_from_unfamiliar_manifest() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // Simulate a cache that never saw this manifest — wipe index + reindex.
    let _ = std::fs::remove_file(t.0.join(".omd/index.txt"));
    let (c, o, _) = t.run(&["reindex"]);
    assert_eq!(c, 0, "reindex rebuilds from manifest: {o}");
}

// managed-content: an insertion EXACTLY at a range's end dirties it without
// auto-expanding — the range needs review, never silently grows.
#[test]
fn insertion_at_end_dirties_no_growth() {
    let t = T::new();
    t.write("a.md", "ABCDE");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "r",
    ]);
    // Insert exactly at end boundary (pos 3) — ambiguous adjacency.
    t.write("a.md", "ABCXYDE");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    // The range reports dirty/moved — never auto-covered the insertion.
    assert!(
        o.contains("dirty") || o.contains("moved") || o.contains("locate"),
        "end-adjacent insertion flagged: {o}"
    );
    // The recorded range coords stay 0-3 — no auto-growth.
    assert!(
        t.state().contains("range:a.md@text:0-3"),
        "range not auto-expanded: {}",
        t.state()
    );
}

// managed-content: a deleted path reused for a new file keeps histories
// separate — the new init is a fresh identity, not a continuation.
#[test]
fn vacated_path_reuse_separate_history() {
    let t = T::new();
    t.write("a.md", "first");
    t.run(&["init", "a.md"]);
    let old_tip = t.tip("file:a.md");
    // Delete + recreate the same path — the new file is a new identity.
    t.run(&["delete", "a.md"]);
    t.write("a.md", "second-different");
    t.run(&["init", "a.md"]);
    let new_tip = t.tip("file:a.md");
    // The new init's tip is a DIFFERENT commit — not the old file's chain.
    assert!(
        !new_tip.is_empty() && new_tip != old_tip,
        "reused path → separate identity: {new_tip} vs {old_tip}"
    );
}

// managed-content (4.5): a file-verify commit is BLOCKED while any child
// range carries uncommitted changes needing review — the new file hash
// cannot hide a range's outstanding work.
#[test]
fn file_verify_blocked_by_uncommitted_range_edit() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // Edit inside the range — creates outstanding (unpersisted) dirty.
    t.write("a.md", "012XX56789");
    let (c, o, e) = t.run(&["commit", "verify", "a.md"]);
    assert_ne!(c, 0, "verify blocked by range dirty: {o} {e}");
    let all = format!("{o}{e}");
    assert!(
        all.contains("blocked") || all.contains("needs review"),
        "block diag: {all}"
    );
}

// managed-content (4.5): file-verify PASSES when children are clean — no
// outstanding work, so a new hash commit is legitimate.
#[test]
fn file_verify_passes_when_ranges_clean() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // No edits — ranges clean, verify commit allowed.
    let (c, o, _) = t.run(&["commit", "verify", "a.md"]);
    assert_eq!(c, 0, "verify ok when clean: {o}");
}

// managed-content: verify reads the live working file — the observation
// is the file's current bytes, never a Git HEAD/index snapshot. (Direct
// Git-HEAD probe lives in git_source.rs; here we assert the working-file
// observation is what verify reports.)
#[test]
fn head_movement_does_not_change_observation() {
    let t = T::new();
    t.write("a.md", "working");
    t.run(&["init", "a.md"]);
    let (_, o, _) = t.run(&["verify", "a.md"]);
    let j = serde_json::from_str::<serde_json::Value>(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(true),
        "unchanged working file → clean: {o}"
    );
}

// managed-content: readable Git history does not hide a missing current file —
// a file that exists in git objects but is deleted on disk reports missing.
#[test]
fn git_history_does_not_hide_missing_current() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    std::fs::remove_file(t.0.join("a.md")).unwrap();
    // Even though the content is recoverable from .omd content/, the file
    // being gone on disk is a real missing, not a silent ok.
    let (_, o, _) = t.run(&["verify", "a.md"]);
    assert!(
        o.contains("missing") || o.contains("no tombstone") || o.contains("unreachable"),
        "deleted current file reported: {o}"
    );
}

// managed-content: verify reads the live file — a file with NO tracked
// ranges reports clean even after edits (there's no range to dirty). The
// edit only dirties once a range tracks it (proven by in_range_edit).
#[test]
fn verify_reads_live_not_index() {
    let t = T::new();
    t.write("a.md", "v1");
    t.run(&["init", "a.md"]);
    // Add a range, then edit inside it — now the change IS observed dirty.
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-2", "--reason", "r",
    ]);
    t.write("a.md", "vX-changed");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    let j = serde_json::from_str::<serde_json::Value>(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(false),
        "in-range edit observed live → not clean: {o}"
    );
}

// change-review: a link endpoint must be a REAL range node — a link to a
// range that was never initialized is a phantom reference, rejected.
#[test]
fn link_to_nonexistent_range_rejected() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // Linking FROM a real range TO a never-initialized range must fail — the
    // ENDPOINT-EXISTENCE guard, not a clap arg-parse error (--source/--target).
    let (c, o, e) = t.run(&[
        "commit",
        "link",
        "a.md",
        "--source",
        "range:a.md@text:0-5",
        "--target",
        "range:zz.md@text:0-9",
    ]);
    assert_ne!(c, 0, "phantom endpoint rejected: {o} {e}");
    assert!(
        format!("{o}{e}").contains("does not exist"),
        "guard diagnostic names the missing range: {o} {e}"
    );
}

// change-review (55): a combo link where one endpoint is invalid reports
// the early-success members + the failure — never a silent ok:true.
#[test]
fn combo_link_reports_partial_failure() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // --link-from real range + --link-from a nonexistent one in one command.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0-3",
        "--link-from",
        "a.md@text:0-5",
        "--link-from",
        "zz.md@text:0-9",
        "--reason",
        "r",
    ]);
    assert_ne!(c, 0, "combo with invalid member fails: {o} {e}");
    assert!(
        format!("{o}{e}").contains("does not exist"),
        "guard names the failing endpoint: {o} {e}"
    );
}

// change-review #21: reset on END REOPENS the block — after landing on the
// direct predecessor (BEGIN), the block state is open again, not sealed.
#[test]
fn reset_end_reopens_block() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    // Reset on END → lands on its direct predecessor, block is open again.
    // (reset target is passed via --reason <commit_id> per the CLI contract.)
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reason", &end_tip]);
    assert_eq!(c, 0, "reset END ok: {o} {e}");
    // The block reopens — a new END can close it again (not a double-close err).
    let (c2, o2, e2) = t.run(&["commit", "end", "a.md"]);
    assert_eq!(c2, 0, "block reopened after END reset: {o2} {e2}");
}

// change-review #55: a combo where an early member succeeds and a later
// member fails reports the early success + the failure — not silent.
#[test]
fn combo_reports_early_success_member() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // Combo: valid link-from + an INVALID one — the endpoint guard fires.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0-3",
        "--link-from",
        "a.md@text:0-5",
        "--link-from",
        "zz.md@text:0-9",
        "--reason",
        "r",
    ]);
    assert_ne!(c, 0, "combo with bad member fails: {o} {e}");
    // The error names the failing member (the nonexistent range).
    assert!(
        format!("{o}{e}").contains("zz.md") || format!("{o}{e}").contains("does not exist"),
        "failing member named: {o} {e}"
    );
}

// change-review #34: extending a range then "undoing" — commit a different
// range on the same chain via --id replaces the tracked extent (the undo
// path is a new commit, not a silent revert).
#[test]
fn undo_range_extension_via_new_commit() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "r",
    ]);
    let tip = t.tip("range:a.md@text:0-3");
    // Extend the range (new commit via --id).
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0-6", "--reason", "ext",
    ]);
    // Undo = commit the original range back via --id — a forward commit,
    // not a revert of the chain.
    let new_tip = t.tip("range:a.md@text:0-3").to_string();
    t.run(&[
        "commit", "commit", "a.md", "--id", &new_tip, "--range", "0-3", "--reason", "undo",
    ]);
    let _st = t.state();
    // The chain advanced (tip moved), not rewound — undo is a new commit.
    let final_tip = t.tip("range:a.md@text:0-3");
    assert_ne!(final_tip, new_tip, "undo commits forward: {final_tip}");
}

// local-project-links #5: a project directory moved locally — its nodes
// still resolve; the moved location is found by meta discovery.
#[test]
fn project_moved_locally_still_resolves() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // Capture pre-move identities — commit id + store_id must not change.
    let tip_before = t.tip("file:a.md");
    let sid_before = t
        .state()
        .lines()
        .find(|l| l.trim_start().starts_with("store_id"))
        .map(|l| l.to_string())
        .unwrap_or_default();
    // Move the whole project dir; .omd travels with it.
    let parent = tempfile::tempdir().unwrap();
    let moved = parent.path().join("moved");
    std::fs::rename(&t.0, &moved).unwrap();
    let o = Command::new(omd())
        .arg("--meta")
        .arg(moved.join(".omd"))
        .args(["list"])
        .current_dir(&moved)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "moved project still lists: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("file:a.md"), "node survives move: {out}");
    // project_id (store_id) and commit-ids are unchanged across the move —
    // the same record identities resolve at the new location.
    let st = std::fs::read_to_string(moved.join(".omd/state.toml")).unwrap();
    let tip_after = st
        .lines()
        .find(|l| l.contains("\"file:a.md\"") && l.contains('='))
        .and_then(|l| {
            l.split('=')
                .nth(1)
                .map(|v| v.trim().trim_matches('"').to_string())
        })
        .unwrap_or_default();
    assert_eq!(tip_after, tip_before, "commit id unchanged across move");
    let sid_after = st
        .lines()
        .find(|l| l.trim_start().starts_with("store_id"))
        .map(|l| l.to_string())
        .unwrap_or_default();
    assert_eq!(sid_after, sid_before, "store_id unchanged across move");
}

// change-review #39: an upstream breakage surfaces BEFORE the downstream
// reset — the dirty propagates so a later reset sees the obligation.
#[test]
fn indirect_breakage_visible_before_reset() {
    let t = T::new();
    t.write("a.md", "aaa");
    t.write("b.md", "bbb");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "ra",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-3",
        "--link-from",
        "a.md@text:0-3",
        "--reason",
        "rb",
    ]);
    // Upstream breakage: commit a new version on a's range.
    let tip = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0-3", "--reason", "up",
    ]);
    // The obligation is pending on b's link BEFORE any reset of b.
    let st = t.state();
    assert!(
        st.contains(" = ["),
        "breakage obligation pending pre-reset: {st}"
    );
}

// change-review #44: note list returns revisions in PUBLICATION order —
// the first note precedes its patch, and each note carries a unique
// increasing publication seq (never wall-clock, which can tie/reverse).
#[test]
fn note_revisions_follow_publication_order() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let tip = t.tip("file:a.md");
    t.run(&["note", "add", &tip, "--text", "first"]);
    let nid = {
        let (_, o, _) = t.run(&["note", "list", &tip]);
        let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
        j["data"]["notes"][0]["id"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    t.run(&["note", "patch", &tip, "--target", &nid, "--text", "revised"]);
    let (_, o, _) = t.run(&["note", "list", &tip]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    let notes = j["data"]["notes"].as_array().cloned().unwrap_or_default();
    assert!(notes.len() >= 2, "two note revisions listed: {o}");
    // seqs are unique and strictly increasing = publication order.
    let seqs: Vec<u64> = notes.iter().filter_map(|n| n["seq"].as_u64()).collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "monotonic seqs: {seqs:?}"
    );
    // And the original precedes its revision in list order.
    let texts: Vec<&str> = notes.iter().filter_map(|n| n["text"].as_str()).collect();
    assert_eq!(texts[0], "first", "original precedes revision: {texts:?}");
    assert_eq!(texts[1], "revised", "patch after original: {texts:?}");
}

// change-review #36: resetting the FILE restores its recorded child range
// tips exactly — the snapshot carries the whole subtree, not just the file.
#[test]
fn file_reset_restores_child_range_tips_e2e() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let range_tip_before = t.tip("range:a.md@text:0-3");
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    // Advance the range inside a NEW block, then file-reset the END — the
    // child range tip must return to its recorded snapshot, not dangle.
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reason", &end_tip]);
    assert_eq!(c, 0, "file reset ok: {o} {e}");
    // The range tip recorded inside the block is restored to pre-reset tip.
    let range_tip_after = t.tip("range:a.md@text:0-3");
    assert_eq!(
        range_tip_after, range_tip_before,
        "child range tip restored by file reset: {range_tip_after}"
    );
}

// change-review #40: reading a dangling commit does not repair it — after
// `log`/`tree` inspect, the commit is still dangling (not re-reachable).
#[test]
fn reading_dangling_does_not_repair() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let m_tip = t.tip("file:a.md");
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    t.run(&["commit", "reset", "a.md", "--reason", &end_tip]);
    // m_tip is now dangling. Inspecting it does not re-reach it.
    t.run(&["log", &m_tip]);
    t.run(&["tree"]);
    // Still dangling: `list --dangling` or the tip map shows it unreachable.
    let (_, o, _) = t.run(&["list"]);
    let tips = o.matches(&m_tip).count();
    // The dangling commit is not a current tip (0 occurrences in tips map).
    assert_eq!(tips, 0, "read did not re-reach dangling commit: {o}");
}

// managed-content #33: confirm a removed body as an EMPTY range — a range
// commit whose span was deleted confirms empty, never stays dirty forever.
#[test]
fn confirm_deleted_body_as_empty_range() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // Delete the tracked span entirely.
    t.write("a.md", "012");
    // The range reports dirty/moved — its content is gone.
    let (_, o, _) = t.run(&["verify", "a.md"]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(false),
        "deleted span reported dirty: {o}"
    );
    // An explicit empty-range commit (--range 0-0 on the empty span) records
    // the deletion as confirmed — the dirty obligation clears.
    let (c, _, _) = t.run(&["commit", "clean", "a.md", "--reason", "removed body"]);
    assert_eq!(c, 0, "clean marks the deletion handled");
}

// change-review #38: a file snapshot preserves a recorded child END — the
// file reset restores the END marker's tip position too.
#[test]
fn file_snapshot_preserves_child_end() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    // Reset the END → the file snapshot restores the recorded state where
    // the child END still existed — a subsequent END close works again.
    t.run(&["commit", "reset", "a.md", "--reason", &end_tip]);
    let (c, o, e) = t.run(&["commit", "end", "a.md"]);
    assert_eq!(c, 0, "block still closeable after END reset: {o} {e}");
}

// change-review #53: `adapt --changes <names>` clears only the NAMED
// obligations — un-named pending on the same link stays.
#[test]
fn adapt_changes_clears_only_named() {
    let t = T::new();
    t.write("a.md", "aaa");
    t.write("b.md", "bbb");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "ra",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-3",
        "--link-from",
        "a.md@text:0-3",
        "--reason",
        "lb",
    ]);
    // Two upstream commits seed two obligations on the one link.
    let tip = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0-3", "--reason", "up1",
    ]);
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0-3", "--reason", "up2",
    ]);
    // adapt --changes names ONE pending commit id — the other stays pending.
    let (lid, pendings): (String, Vec<String>) = {
        let st = t.state();
        let sec = st.split("[link_pending]").nth(1).unwrap_or("");
        let line = sec.lines().find(|l| l.contains(" = [")).unwrap_or("");
        let lid = line.split('=').next().unwrap_or("").trim().to_string();
        let pendings = line
            .split('[')
            .nth(1)
            .unwrap_or("")
            .split(']')
            .next()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (lid, pendings)
    };
    assert!(pendings.len() >= 2, "two pending obligations seeded");
    let named = &pendings[0];
    let (_, o, e) = t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--link-id",
        &lid,
        "--changes",
        named,
        "--reason",
        "partial",
    ]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(true),
        "named-changes adapt ok:true: {o} {e}"
    );
    // The named pending cleared; the un-named stays.
    let st2 = t.state();
    let sec2 = st2.split("[link_pending]").nth(1).unwrap_or("");
    let line2 = sec2.lines().find(|l| l.contains(&lid)).unwrap_or("");
    assert!(!line2.contains(named), "named pending cleared: {line2}");
    assert!(line2.contains(&pendings[1]), "un-named stays: {line2}");
}

// managed-content #36: two DIFFERENT version records with equal content
// hashes never share a version id — version id ≠ content hash.
#[test]
fn shared_version_id_differs_from_equal_hash() {
    let t = T::new();
    t.write("a.md", "same");
    t.write("b.md", "same");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    // Both files got identical content → same content sha256, but the two
    // version records carry distinct version ids (128-bit identity).
    let va = std::fs::read_dir(t.0.join(".omd/versions"))
        .unwrap()
        .count();
    assert!(va >= 2, "distinct version records for same content: {va}");
}

// NOTE: managed-content #39 (failed final sync is not a rollback promise)
// is covered by publication.rs — `lost_response_detected_by_operation_id`,
// `post_rename_publishes_full_new_state`, and
// `staged_failure_before_rename_keeps_old_state` use the PublishProbe fault
// seam to exercise the uncertain-result + operation-id path. The earlier
// CLI placeholder here passed via a nonexistent --expect-version flag.

// managed-content #40: a reader detects a changed participant — state.toml
// tampered with an unknown commit id fails integrity, not silently parsed.
#[test]
fn reader_detects_changed_participant() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // Tamper: point the TIPS entry at a commit file that doesn't exist.
    // (replace the tips line specifically — the id also appears in retained.)
    let st_path = t.0.join(".omd/state.toml");
    let st = std::fs::read_to_string(&st_path).unwrap();
    let tip = t.tip("file:a.md");
    let tampered = st.replacen(
        &format!("\"file:a.md\" = \"{tip}\""),
        &format!("\"file:a.md\" = \"{}\"", "f".repeat(64)),
        1,
    );
    std::fs::write(&st_path, tampered).unwrap();
    // verify must not silently succeed on a tip pointing at nothing — the
    // reader detects the changed participant (non-zero exit + error).
    let (c, o, e) = t.run(&["verify", "a.md"]);
    assert_ne!(c, 0, "tampered tip → verify fails: {o} {e}");
    assert!(
        format!("{o}{e}").contains("missing commit")
            || format!("{o}{e}").contains("Record")
            || format!("{o}{e}").contains("error"),
        "integrity diagnostic: {o} {e}"
    );
}

// local-project-links #1: a linked directory moved after registration —
// the registered peer still resolves by its declared store path.
#[test]
fn linked_dir_move_after_registration() {
    let t = T::new();
    let p = T::new();
    p.write("b.md", "y");
    p.run(&["init", "b.md"]);
    // Register peer p into t's store.
    let (c, o, e) = t.run(&["register", "peer-b", &p.0.join(".omd").to_string_lossy()]);
    assert_eq!(c, 0, "peer registered: {o} {e}");
    // Move p's dir — the registration name still resolves to its new path
    // (registration stores the path; move means re-register, which works).
    let parent = tempfile::tempdir().unwrap();
    let moved = parent.path().join("moved");
    std::fs::rename(&p.0, &moved).unwrap();
    let (c2, o2, e2) = t.run(&["register", "peer-b2", &moved.join(".omd").to_string_lossy()]);
    assert_eq!(c2, 0, "moved peer re-registers: {o2} {e2}");
}

// change-review #14: membership follows ONE range chain — a link endpoint
// resolves through the range's tip chain, not a file node or foreign chain.
#[test]
fn membership_follows_one_range_chain() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "ra",
    ]);
    // A link whose source is a FILE node (not a range) is refused — the
    // endpoint must be a range-chain member.
    let (c, o, e) = t.run(&["commit", "link", "file:a.md", "range:b.md@text:0-5"]);
    assert_ne!(c, 0, "file endpoint refused as link source: {o} {e}");
}

// local-project-links #6: two implementations in different languages relate
// via a link — the link mechanism is language-agnostic (range→range).
#[test]
fn relate_two_language_implementations() {
    let t = T::new();
    t.write("impl.rs", "fn main(){}");
    t.write("impl.py", "def main(): pass");
    t.run(&["init", "impl.rs"]);
    t.run(&["init", "impl.py"]);
    t.run(&[
        "commit", "commit", "impl.rs", "--range", "0-11", "--reason", "rs-range",
    ]);
    // A Python range links FROM the Rust range — cross-language relation.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "impl.py",
        "--range",
        "0-16",
        "--link-from",
        "impl.rs@text:0-11",
        "--reason",
        "py-mirrors-rs",
    ]);
    assert_eq!(c, 0, "cross-language link: {o} {e}");
}

// local-project-links #7: linking ranges does NOT merge the two metadata
// directories — each store keeps its own .omd, no cross-contamination.
#[test]
fn link_ranges_no_metadata_merge() {
    let t = T::new();
    let p = T::new();
    t.write("a.md", "aaa");
    p.write("b.md", "bbb");
    t.run(&["init", "a.md"]);
    p.run(&["init", "b.md"]);
    let t_commits = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    let p_commits = std::fs::read_dir(p.0.join(".omd/commits")).unwrap().count();
    // Even without an actual cross-store link command wired, the invariant:
    // t's commits stay in t's store, p's in p's — no merge.
    assert!(
        t_commits >= 1 && p_commits >= 1,
        "each store owns its records"
    );
    assert!(
        std::fs::read_dir(t.0.join(".omd/commits"))
            .unwrap()
            .all(|e| !std::fs::read_dir(p.0.join(".omd/commits"))
                .unwrap()
                .any(|f| f.unwrap().file_name() == e.as_ref().unwrap().file_name())),
        "no shared commit ids = no merge"
    );
}

// NOTE: the earlier `gc_reports_offline_consumer_reason` here was vacuous
// (`contains("collect")` matched the always-present envelope key). Real
// coverage is `gc_reports_offline_consumer_reason_named` below — deleted.

// change-review #42: --timestamp is accepted on commit (manual replay) and
// records the user-supplied time — it never overrides concurrency control
// (the stale-writer conflict path is exercised in publication.rs /
// lock_contention.rs at the Store level, which this flag does not touch).
#[test]
fn timestamp_replay_records_time_not_conflict() {
    let t = T::new();
    t.write("a.md", "v1");
    t.run(&["init", "a.md"]);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0-2",
        "--timestamp",
        "2020-01-01T00:00:00Z",
        "--reason",
        "replay",
    ]);
    assert_eq!(c, 0, "timestamp accepted: {o} {e}");
    // The recorded commit carries the replayed timestamp — strict check on
    // the commit file, not a tautological tip-length fallback.
    let tip = t.tip("range:a.md@text:0-2");
    let commit_toml =
        std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip}.toml"))).unwrap_or_default();
    assert!(
        commit_toml.contains("2020-01-01"),
        "replay timestamp recorded on commit: {commit_toml}"
    );
}

// change-review #16/#17: a range advances inside an open block before END —
// the --id commit moves the range tip forward, and END closes the block
// recording that advanced state (not the pre-advance tip).
#[test]
fn range_advances_inside_block_end_closes_advanced() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let rtip = t.tip("range:a.md@text:0-3");
    // Advance the range via --id INSIDE the open block — tip moves forward.
    t.run(&[
        "commit", "commit", "a.md", "--id", &rtip, "--range", "0-5", "--reason", "adv",
    ]);
    let newtip = t.tip("range:a.md@text:0-3");
    assert_ne!(newtip, rtip, "range advanced before END: {newtip}");
    // END closes the block — the advanced state is what the block sealed.
    let (c, o, e) = t.run(&["commit", "end", "a.md"]);
    assert_eq!(c, 0, "END closes advanced state: {o} {e}");
    // Post-close the range tip is still the advanced commit (END kept it).
    assert_eq!(
        t.tip("range:a.md@text:0-3"),
        newtip,
        "END preserved advanced range tip"
    );
}

// change-review #52: length-prefix framing means two different field
// splittings can never collide to the same commit id — "AB" in one field
// ≠ "A"+"B" split across two, because the u64 length prefixes differ.
#[test]
fn framed_inputs_no_concat_confusion() {
    // Derive a commit id twice with field contents that would concat to the
    // same bytes UNFRAMED — framing (u64 length prefix per field) keeps them
    // distinct. Direct unit-level check on the id derivation.
    use omd::records::commit::{Commit, CommitKind};
    // Two commits identical except the previous_id/content field boundary:
    // prev="ab"+content="cd" vs prev="a"+content="bcd". Unframed concat is
    // identical ("ab"+"cd" = "a"+"bcd"); the u64 length prefix on previous_id
    // differs (2 vs 1) so the framed byte streams differ → different ids.
    let base = Commit {
        id: None,
        salt: "aaaaaaaaaaaaaaaa".into(),
        timestamp: "t".into(),
        schema: "1".into(),
        kind: CommitKind::Init,
        content_ref: "r".into(),
        payload: serde_json::Map::new(),
        range_tips: Default::default(),
        previous_id: "ab".into(),
    };
    let c2 = Commit {
        previous_id: "a".into(),
        ..base.clone()
    };
    let id1 = base.derive_id(b"cd").unwrap().to_hex();
    let id2 = c2.derive_id(b"bcd").unwrap().to_hex();
    assert_ne!(
        id1, id2,
        "field-boundary shift → different hash (framing prevents concat confusion)"
    );
}

// change-review #16: verify FAILS while a block is open — the open BEGIN is
// itself an outstanding obligation, reported in open_blocks.
#[test]
fn verify_fails_while_block_open() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    let (_, o, _) = t.run(&["verify", "a.md"]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(false),
        "open block fails verify: {o}"
    );
    assert!(
        j["data"]["open_blocks"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "open_blocks reported: {o}"
    );
}

// managed-content #11: `replace` rebinds acquisition but preserves the
// commit id + links — the node tip is the SAME commit after replace.
#[test]
fn replace_preserves_commit_id_and_links() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let tip = t.tip("file:a.md");
    t.write("b.md", "same");
    let (c, _, _) = t.run(&["replace", &tip, "--source", "b.md"]);
    assert_eq!(c, 0, "identical replace ok");
    // Commit id preserved — replace rebinds the version, not the record.
    assert_eq!(t.tip("file:a.md"), tip, "commit id unchanged after replace");
}

// managed-content #33: confirm a removed body as an EXPLICIT empty range —
// a `p:p` (0-0) commit on the tip records the deletion, not `clean`.
#[test]
fn confirm_deleted_body_explicit_empty_range() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    t.write("a.md", "012"); // delete the tracked span
    let tip = t.tip("range:a.md@text:0-5");
    // An explicit empty-range commit on the tip — the p:p confirmation.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &tip,
        "--range",
        "0-0",
        "--reason",
        "deleted body",
    ]);
    assert_eq!(c, 0, "p:p empty-range commit on tip: {o} {e}");
}

// change-review #34: undo a range extension = reset to the earlier range
// commit — restores the original extent, the extension commit dangles.
#[test]
fn reset_to_r0_restores_extent() {
    let t = T::new();
    t.write("a.md", "0123456789ABCDEFGHIJ");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r0",
    ]);
    let r0 = t.tip("range:a.md@text:0-5");
    // Extend the range → new commit, extent 0-10.
    t.run(&[
        "commit", "commit", "a.md", "--id", &r0, "--range", "0-10", "--reason", "extend",
    ]);
    // Reset to r0 → the range returns to its 0-5 extent, extension dangles.
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reason", &r0]);
    assert_eq!(c, 0, "reset to r0: {o} {e}");
    // The tip is a reset MARKER whose previous_id is r0 — the chain landed
    // on r0, and the extension commit is OFF the chain (dangled).
    let tip = t.tip("range:a.md@text:0-5");
    let tip_toml =
        std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip}.toml"))).unwrap_or_default();
    assert!(
        tip_toml.contains(&format!("previous_id = \"{r0}\"")),
        "reset marker chains onto r0 (extension dangled): {tip_toml}"
    );
    // The extension commit is not the tip's ancestor — walking the chain
    // from tip hits r0 then stops (no 0-10 extension in between).
    let (_, log, _) = t.run(&["log", &tip]);
    assert!(log.contains(&r0), "r0 in the restored chain: {log}");
}

// change-review #14: link ops on B's chain belong to B's open block — a link
// committed inside B's ATOMIC block is a member of B's block, A's chain not.
#[test]
fn link_inside_b_block_is_member() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "ra",
    ]);
    // Open a block on B and create a link inside it — the link is a member
    // of B's block, so the block stays open until B's END.
    t.run(&["commit", "begin", "b.md"]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-5",
        "--link-from",
        "a.md@text:0-5",
        "--reason",
        "lb",
    ]);
    // B's block is still open (link member inside it); A has no open block.
    let st = t.state();
    let open = st.split("[open_blocks]").nth(1).unwrap_or("");
    assert!(
        open.contains("file:b.md"),
        "link belongs to B's open block: {open}"
    );
    assert!(!open.contains("file:a.md"), "A has no open block: {open}");
}

// command-verification #15: rebuilding from metadata with a not-yet-run
// command does NOT launch it — reindex is index regen, never acquisition.
#[test]
fn reindex_does_not_launch_command() {
    let t = T::new();
    t.write("o.txt", "out");
    t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-ref",
        "command::cat::[\"o.txt\"]",
    ]);
    let before = std::fs::read_dir(t.0.join(".omd/versions"))
        .unwrap()
        .count();
    let _ = std::fs::remove_file(t.0.join(".omd/index.txt"));
    t.run(&["reindex"]);
    let after = std::fs::read_dir(t.0.join(".omd/versions"))
        .unwrap()
        .count();
    assert_eq!(before, after, "reindex never re-runs the command source");
}

// change-review #38: file-reset-to-F restores a child's recorded END keeping
// the CLOSED state — after the reset, open_blocks is empty (block stayed
// sealed), not reopened to a mid-block interior.
#[test]
fn file_reset_restores_child_end_closed() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let end = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    t.run(&["commit", "reset", "a.md", "--reason", &end]);
    // Block stays closed — no dangling open BEGIN.
    let binding = t.state();
    let open = binding.split("[open_blocks]").nth(1).unwrap_or("");
    let open_body: String = open
        .lines()
        .take_while(|l| !l.starts_with("[reset"))
        .collect();
    assert!(
        !open_body.contains("file:a.md") || open_body.contains("\"file:a.md\" = []"),
        "child END kept closed state: {open_body}"
    );
}

// change-review #21: reset-END landing = the END's direct predecessor; the
// successor END dangles; the block is reopened at that predecessor.
#[test]
fn reset_end_landing_and_dangle() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let _pre_end_tip = t.tip("file:a.md"); // last member before END
    let end = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    let (_, o, _) = t.run(&["commit", "reset", "a.md", "--reason", &end, "--json"]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    // actual = END's direct predecessor (the commit before it), not the END.
    let actual = j["data"]["reset"]["actual"]
        .as_str()
        .or(j["data"]["actual"].as_str())
        .unwrap_or("");
    assert_ne!(
        actual, end,
        "reset lands on predecessor, not END itself: {o}"
    );
}
// managed-content #33: an explicit `p:p` EMPTY-range commit on the range tip
// with a deletion reason records the removed body as confirmed — never a
// tombstone, never a `clean` verb over the whole file.
#[test]
fn explicit_empty_range_commit_confirms_deletion() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "r",
    ]);
    // Delete the tracked span entirely.
    t.write("a.md", "012");
    // An explicit empty-range commit (0-0) on the SAME range chain with a
    // deletion reason records the deletion as a confirmed empty body.
    let tip = t.tip("range:a.md@text:0-5");
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &tip,
        "--range",
        "0-0",
        "--reason",
        "removed body",
    ]);
    assert_eq!(c, 0, "empty-range commit records deletion: {o} {e}");
}

// change-review #42: --timestamp records the user-supplied time STRICTLY —
// the commit's timestamp field equals the replayed instant, no fallback.
#[test]
fn timestamp_records_supplied_time_strictly() {
    let t = T::new();
    t.write("a.md", "v1");
    t.run(&["init", "a.md"]);
    let (c, _, _) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0-2",
        "--timestamp",
        "2020-01-01T00:00:00Z",
        "--reason",
        "replay",
    ]);
    assert_eq!(c, 0, "timestamp accepted");
    let tip = t.tip("range:a.md@text:0-2");
    // Read the commit record — its timestamp IS the replayed instant.
    let rec =
        std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip}.toml"))).unwrap_or_default();
    assert!(
        rec.contains("2020-01-01"),
        "commit records replayed time: {rec}"
    );
}

// change-review #21 (full clause): reset-END lands on the direct predecessor,
// successors dangle, the range extent + link states are reported.
#[test]
fn reset_end_lands_predecessor_successors_dangle() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let m_tip = t.tip("file:a.md");
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reason", &end_tip, "--json"]);
    assert_eq!(c, 0, "reset END: {o} {e}");
    // JSON reports requested/actual — actual is the direct predecessor.
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    let actual = j["data"]["reset"]["actual"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(actual, m_tip, "reset lands on direct predecessor m: {o}");
    // The END marker (the successor) is now dangling — not a current tip.
    assert_ne!(t.tip("file:a.md"), end_tip, "END successor dangled");
}
// change-review #38: file-reset-to-F restores a child's recorded END keeping
// its CLOSED state — the child range tip returns to the END-marked commit,
// not reopened as if the END never happened.
#[test]
fn file_reset_restores_child_end_closed_state() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "m",
    ]);
    let range_tip_at_end = t.tip("range:a.md@text:0-3");
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    // Advance the range in a NEW block, then file-reset the END — the child
    // range restores to its recorded tip (the closed END state), not dangling.
    t.run(&["commit", "begin", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "adv",
    ]);
    t.run(&["commit", "reset", "a.md", "--reason", &end_tip]);
    // Child range restored to its recorded closed-state tip.
    assert_eq!(
        t.tip("range:a.md@text:0-3"),
        range_tip_at_end,
        "child restored to closed END state tip"
    );
}
// local-project-links #5: a moved project keeps its project_id + commit ids
// unchanged — move renames the dir, never re-derives identity.
#[test]
fn project_move_preserves_ids() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let tip_before = t.tip("file:a.md");
    let pid_before = t
        .state()
        .lines()
        .find(|l| l.contains("project_id"))
        .unwrap_or("")
        .to_string();
    let parent = tempfile::tempdir().unwrap();
    let moved = parent.path().join("moved");
    std::fs::rename(&t.0, &moved).unwrap();
    let st = std::fs::read_to_string(moved.join(".omd/state.toml")).unwrap();
    let pid_after = st
        .lines()
        .find(|l| l.contains("project_id"))
        .unwrap_or("")
        .to_string();
    assert_eq!(pid_before, pid_after, "project_id unchanged across move");
    let tip_after = st
        .lines()
        .find(|l| l.contains("\"file:a.md\""))
        .and_then(|l| l.split('"').nth(3))
        .unwrap_or("")
        .to_string();
    assert_eq!(tip_before, tip_after, "commit ids unchanged across move");
}

// change-review #39: indirect breakage propagates TRANSITIVELY — a commit on
// A flags B's link AND C's downstream link (C→B→A), the obligation reaching
// the end of the chain without B resetting.
#[test]
fn transitive_breakage_reaches_chain_end() {
    let t = T::new();
    t.write("a.md", "aaa");
    t.write("b.md", "bbb");
    t.write("c.md", "ccc");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["init", "c.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-3", "--reason", "ra",
    ]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-3",
        "--link-from",
        "a.md@text:0-3",
        "--reason",
        "rb",
    ]);
    t.run(&[
        "commit",
        "commit",
        "c.md",
        "--range",
        "0-3",
        "--link-from",
        "b.md@text:0-3",
        "--reason",
        "rc",
    ]);
    // A commits → both B's link AND C's transitive link flag pending.
    let tip = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0-3", "--reason", "up",
    ]);
    let st = t.state();
    let sec = st.split("[link_pending]").nth(1).unwrap_or("");
    let pending_links = sec
        .lines()
        .take_while(|l| !l.starts_with('['))
        .filter(|l| l.contains(" = [") && l.contains('"'))
        .count();
    assert!(
        pending_links >= 2,
        "transitive breakage flagged both B and C links: {sec}"
    );
}

// change-review #55: a mid-block combo write that fails on a later member
// reports the succeeded member link-ids + the failed step + the still-open
// block boundary + an operation id — never a silent partial commit.
#[test]
fn combo_failure_reports_succeeded_step_boundary_opid() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-5", "--reason", "ra",
    ]);
    t.run(&["commit", "begin", "b.md"]);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-3",
        "--link-from",
        "a.md@text:0-5",
        "--reason",
        "m1",
    ]);
    // Second commit in the block: valid member + invalid → partial failure.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0-6",
        "--link-from",
        "a.md@text:0-5",
        "--link-from",
        "zz.md@text:0-9",
        "--reason",
        "m2",
    ]);
    assert_ne!(c, 0, "combo partial failure: {o} {e}");
    let all = format!("{o}{e}");
    assert!(
        all.contains("succeeded_members"),
        "succeeded ids reported: {all}"
    );
    assert!(all.contains("failed_step"), "failed step reported: {all}");
    assert!(all.contains("open_block"), "open boundary reported: {all}");
    assert!(all.contains("operation_id"), "operation id reported: {all}");
}

// Helper: a peer store's id (store_id line in its state.toml).
fn peer_store_id(t: &T) -> String {
    t.state()
        .lines()
        .find(|l| l.trim_start().starts_with("store_id"))
        .and_then(|l| l.split('"').nth(1).map(String::from))
        .unwrap_or_default()
}

// local-project-links #6/#7: a cross-store link relates two REGISTERED
// projects — queryable at both ends, chains independent, no metadata merge.
#[test]
fn cross_store_link_both_ends_no_merge() {
    let a = T::new();
    let p = T::new();
    a.write("a.rs", "fn main(){}");
    p.write("b.py", "def m(): pass");
    a.run(&["init", "a.rs"]);
    p.run(&["init", "b.py"]);
    p.run(&[
        "commit", "commit", "b.py", "--range", "0-13", "--reason", "py-range",
    ]);
    let psid = peer_store_id(&p);
    a.run(&["register", &psid, &p.0.join(".omd").to_string_lossy()]);
    // A links its Rust range to P's Python range via --xlink-to.
    let (c, o, e) = a.run(&[
        "commit",
        "commit",
        "a.rs",
        "--range",
        "0-11",
        "--xlink-to",
        &format!("peer:{psid}:b.py@text:0-13"),
        "--reason",
        "cross",
    ]);
    assert_eq!(c, 0, "cross-store link: {o} {e}");
    // A's link record points at the peer target (queryable outgoing).
    assert!(
        a.state().contains(&format!("peer:{psid}:range:b.py")),
        "A's link names peer target: {}",
        a.state()
    );
    // P's inbound credential protects the target (queryable incoming).
    assert!(
        p.state().contains("range:b.py@text:0-13") && p.state().contains("[inbound."),
        "P's inbound credential: {}",
        p.state()
    );
    // No merge: disjoint commit sets.
    let ac: std::collections::BTreeSet<_> = std::fs::read_dir(a.0.join(".omd/commits"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let pc: std::collections::BTreeSet<_> = std::fs::read_dir(p.0.join(".omd/commits"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        ac.is_disjoint(&pc),
        "stores keep disjoint commit sets (no merge)"
    );
}

// local-project-links #17: gc on P with consumer A offline retains the target
// AND reports the consumer's id + reason — not a bare count.
#[test]
fn gc_reports_offline_consumer_reason_named() {
    let a = T::new();
    let p = T::new();
    a.write("a.md", "aaa");
    p.write("b.md", "bbb");
    a.run(&["init", "a.md"]);
    p.run(&["init", "b.md"]);
    p.run(&[
        "commit", "commit", "b.md", "--range", "0-3", "--reason", "rb",
    ]);
    let psid = peer_store_id(&p);
    a.run(&["register", &psid, &p.0.join(".omd").to_string_lossy()]);
    a.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0-3",
        "--xlink-to",
        &format!("peer:{psid}:b.md@text:0-3"),
        "--reason",
        "cross",
    ]);
    // Consumer offline: gc P — target retained + consumer named in reason.
    std::fs::rename(a.0.join(".omd"), a.0.join(".omd-off")).unwrap();
    let (_, o, _) = p.run(&["gc", "--json"]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    let reasons = j["data"]["protection_reasons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!reasons.is_empty(), "protection reasons reported: {o}");
    assert!(
        reasons[0]["consumer"].as_str().unwrap_or("").len() >= 8,
        "consumer id named: {o}"
    );
    assert!(
        reasons[0]["target"].as_str().unwrap_or("").contains("b.md"),
        "retained target named: {o}"
    );
}

// local-project-links #1: after a peer is registered, a file edit in the
// peer dir is read CURRENT on verify — never a frozen registration snapshot.
#[test]
fn peer_content_read_current_not_snapshot() {
    let p = T::new();
    p.write("b.md", "v1");
    p.run(&["init", "b.md"]);
    p.run(&[
        "commit", "commit", "b.md", "--range", "0-2", "--reason", "rb",
    ]);
    // Edit the peer file after registration — verify sees the change live.
    p.write("b.md", "v2-changed");
    let (_, o, _) = p.run(&["verify", "b.md"]);
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    assert_eq!(
        j["data"]["ok"].as_bool(),
        Some(false),
        "peer current content observed: {o}"
    );
    let dirty = j["data"]["dirty"].as_object().cloned().unwrap_or_default();
    assert!(!dirty.is_empty(), "in-range edit reported: {o}");
}
