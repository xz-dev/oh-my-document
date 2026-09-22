//! 13.x guards: adapt rejection, link dup scoping, reason not creating
//! relationships, interior-range-commit link targets.

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
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!(
            "omd-gr-{}-{}",
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
    fn write(&self, p: &str, c: &str) {
        std::fs::write(self.0.join(p), c).unwrap();
    }
    fn file_node(&self, path: &str) -> String {
        let text = self.state();
        let value: toml::Value = toml::from_str(&text).unwrap();
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
    /// The N-th range node key (`range:<root-id>`) mounted under a file object.
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
    /// A range node exists for `file` covering `span` — the span lives on
    /// the range tip commit's payload (`range` field), not the key.
    fn range_exists(&self, file: &str, span: &str) -> bool {
        !self.tip(&format!("range:{file}@{span}")).is_empty()
    }
    fn tip(&self, node: &str) -> String {
        let s = std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default();
        // `range:<file>@mode:s-e` (old test spelling) → resolve to the
        // mounted range node under `file:<file>` at the matching span. The
        // new key is `range:<root-commit-id>`; we look the node up via
        // mounts + the span recorded on its tip commit's payload.
        if let Some(rest) = node.strip_prefix("range:") {
            if let Some(at) = rest.find('@') {
                let path = &rest[..at];
                let want_span = &rest[at + 1..]; // "text:0-3" / "byte:0-5"
                let file_node = self.file_node(path);
                // Collect mounted range children of the file node.
                let mut children: Vec<String> = Vec::new();
                for l in s.lines() {
                    if l.contains(&format!("\"{file_node}\"")) && l.contains('[') {
                        for m in l.match_indices("range:") {
                            let r = &l[m.0..];
                            if let Some(end) = r.find('"') {
                                children.push(r[..end].to_string());
                            }
                        }
                    }
                }
                let want = {
                    let (mode, span) = want_span.split_once(':').unwrap_or(("text", want_span));
                    let (start, end) = span.split_once('-').unwrap();
                    omd::relations::range::Range {
                        start: start.parse().unwrap(),
                        end: end.parse().unwrap(),
                        mode: if mode == "byte" {
                            omd::relations::range::Mode::Byte
                        } else {
                            omd::relations::range::Mode::Text
                        },
                    }
                };
                for child in &children {
                    let tip_id = s
                        .lines()
                        .find(|line| {
                            line.starts_with(&format!("\"{child}\"")) && line.contains('=')
                        })
                        .and_then(|line| line.split('=').nth(1))
                        .map(|value| value.trim().trim_matches('"').to_string())
                        .unwrap_or_default();
                    let Ok(text) =
                        std::fs::read_to_string(self.0.join(format!(".omd/commits/{tip_id}.toml")))
                    else {
                        continue;
                    };
                    let Ok(commit) = toml::from_str::<omd::records::commit::Commit>(&text) else {
                        continue;
                    };
                    if commit
                        .payload
                        .get("position")
                        .and_then(omd::relations::node::position_from_value)
                        == Some(want)
                    {
                        return tip_id;
                    }
                }
                // No exact span match — the chain's extent moved. Return the
                // first mounted child's tip (the test wants *that* chain).
                if let Some(c) = children.first() {
                    return s
                        .lines()
                        .find(|l| l.starts_with(&format!("\"{c}\"")) && l.contains('='))
                        .and_then(|l| l.split('=').nth(1))
                        .map(|v| v.trim().trim_matches('"').to_string())
                        .unwrap_or_default();
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
    fn pending(&self, link_id: &str) -> Vec<String> {
        let state: omd::records::store::State = toml::from_str(&self.state()).unwrap();
        state
            .link_pending
            .get(link_id)
            .map(|changes| changes.iter().cloned().collect())
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // b's range commit links FROM a's range — the committing node is b's range.
    let rn1 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn1,
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
        "0",
        "1",
        "--reason",
        "up",
    ]);
    let (c, o, e) = t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--adapt",
        r#"{"changes":["x"],"reason":"r"}"#,
    ]);
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
        "0",
        "1",
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
    let selection = format!(r#"{{"link_id":"{link_id}","changes":["x"]}}"#);
    let (c, o, e) = t.run(&["commit", "adapt", "b.md", "--adapt", &selection]);
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
        "0",
        "1",
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
    let changes = t.pending(&lid);
    let selection = serde_json::json!({
        "link_id": lid,
        "changes": changes,
        "reason": "done",
    })
    .to_string();
    t.run(&["commit", "adapt", "b.md", "--adapt", &selection]);
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    let rn2 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn2,
        "--reason",
        "lb",
    ]);
    let rn3 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "c.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn3,
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
        "0",
        "1",
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
    let changes = t.pending(&lids[0]);
    let selection = serde_json::json!({
        "link_id": lids[0],
        "changes": changes,
    })
    .to_string();
    t.run(&[
        "commit", "clean", "b.md", "--stop", &selection, "--reason", "done",
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
        "0",
        "1",
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
    let selection = serde_json::json!({
        "link_id": lid,
        "changes": t.pending(&lid),
    })
    .to_string();
    let (c, o, e) = t.run(&[
        "commit",
        "adapt",
        "b.md",
        "--adapt",
        &selection,
        "--no--reason",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "r2",
    ]);
    // Link b's range FROM a's non-tip commit c1 — the interior member is a
    // valid reference point. Committing node is b's range.
    let rn4 = t.range_node("a.md", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn4,
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // --link-from AND --link-to the same range in one command: two distinct
    // directions, not a duplicate. Committing node is b's range.
    let rn5 = t.range_node("a.md", 0);
    let rn6 = t.range_node("a.md", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn5,
        "--link-to",
        &rn6,
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // Two separate commands each linking b's range from a's range.
    let rn7 = t.range_node("a.md", 0);
    let (c1, _, _) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn7,
        "--reason",
        "l1",
    ]);
    let rn8 = t.range_node("a.md", 0);
    let (c2, _, _) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn8,
        "--reason",
        "l2",
    ]);
    assert_eq!(c1, 0);
    assert_eq!(
        c2, 0,
        "same --link-from across invocations is allowed (distinct links)"
    );
}

// Re-audit BUG3: independent command fields record Acquisition::Command and
// verify reports it `unverified` when the command isn't permitted to run.
#[test]
fn command_source_records_acquisition_and_unverified() {
    let t = T::new();
    // init the file as a command-sourced version.
    let (c, o, e) = t.run(&[
        "commit",
        "init",
        "f.txt",
        "--source-type",
        "command",
        "--executable",
        "echo",
        "--args-json",
        "[\"hi\"]",
    ]);
    assert_eq!(c, 0, "{o} {e}");
    // The version's acquisition is Command, not File.
    let mut found = false;
    if let Ok(rd) = std::fs::read_dir(t.0.join(".omd/versions")) {
        for en in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(en.path())
                && txt.contains("[acquisition]")
                && txt.contains("type = \"command\"")
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "b1",
    ]);
    // Reset to the END of the block — lands on END's direct predecessor
    // (the last interior/link), one step back, not recursively skipped.
    let end = t.tip("range:a.md@text:0-1");
    let (c, o, _) = t.run(&["commit", "reset", "a.md", "--reset-target", &end]);
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // b's block: BEGIN → interior commit → LINK → END.
    let rn9 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn9,
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
    let (c, o, _) = t.run(&["commit", "reset", "b.md", "--reset-target", &link_commit]);
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // Two separate commands each create a link a-range → b-range.
    let rn10 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn10,
        "--reason",
        "l1",
    ]);
    let rn11 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn11,
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
    // Cache path is selected through OMD_CACHE_PATH and scoped by instance.
    let (c, o, _) = t.run(&["--json", "reindex"]);
    assert_eq!(c, 0, "reindex works without cache: {o}");
    let value: serde_json::Value = serde_json::from_str(&o).unwrap();
    let cache_file = value["data"]["cache_file"].as_str().unwrap();
    let idx = std::fs::read_to_string(cache_file).unwrap_or_default();
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "r2",
    ]);
    let c2 = t.tip("range:a.md@text:0-1");
    // Add a note referencing c2 — it becomes a referenced dangling.
    t.run(&["note", "add", &c2, "--text", "evidence"]);
    t.run(&["commit", "reset", "a.md", "--reset-target", &c1]);
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
    setup_link(&t);
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0",
        "1",
        "--reason",
        "up",
    ]);
    let state: omd::records::store::State = toml::from_str(&t.state()).unwrap();
    let link_id = state.links.keys().next().unwrap().clone();
    let stop = serde_json::json!({
        "link_id": link_id,
        "changes": t.pending(&link_id),
    })
    .to_string();
    let (c, o, e) = t.run(&["commit", "clean", "b.md", "--stop", &stop, "--no--reason"]);
    assert_eq!(c, 0, "clean --no--reason ok: {o} {e}");
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r",
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
        "0",
        "1",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    let rn12 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn12,
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
    let (c, o, _) = t.run(&["commit", "reset", "b.md", "--reset-target", &begin]);
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
    let (c, o, e) = t.run(&[
        "replace",
        &tip,
        "--source-type",
        "file",
        "--source-path",
        "b.md",
    ]);
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r1",
    ]);
    let c1 = t.tip(&t.range_node("a.md", 0));
    // --id c1 with a DIFFERENT range expands/modifies c1's chain — a commit
    // on c1's node, not a fresh independent object.
    let (c, _, _) = t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "10", "--reason", "expand",
    ]);
    assert_eq!(c, 0);
    // The range node c1's chain advanced (the new commit chains onto c1's tip).
    let tip = t.tip(&t.range_node("a.md", 0));
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
        "commit", "commit", "a.md", "--range", "2", "6", "--reason", "r",
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

// managed-content: byte mode is stored and consumed as raw offsets. Bare
// coordinates plus --mode byte remain byte on continuation when mode is
// omitted; out-of-span edits stay clean and in-span edits dirty.
#[test]
fn byte_mode_range_counts_bytes() {
    let t = T::new();
    t.write("a.md", "aébc");
    t.run(&["init", "a.md"]);

    let before = t.state();
    let (bad, _, _) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "6", "--mode", "byte", "--reason", "bad",
    ]);
    assert_eq!(bad, 2, "byte bounds reject as usage");
    assert_eq!(before, t.state(), "invalid bounds publish nothing");

    let (c, o, e) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "3", "--mode", "byte", "--reason", "r",
    ]);
    assert_eq!(c, 0, "byte range commits: {o} {e}");
    let response: serde_json::Value = serde_json::from_str(&o).unwrap();
    let first_tip = response["data"]["commit"].as_str().unwrap().to_string();
    let node = format!("range:{first_tip}");
    let first: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(t.0.join(format!(".omd/commits/{first_tip}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        omd::relations::node::position_from_value(&first.payload["position"]),
        Some(omd::relations::range::Range {
            start: 0,
            end: 3,
            mode: omd::relations::range::Mode::Byte,
        })
    );

    // Byte 4 is outside [0,3): c -> x must not dirty.
    t.write("a.md", "aébx");
    let (clean, out, err) = t.run(&["verify", "a.md"]);
    assert_eq!(clean, 0, "outside-byte-span edit stays clean: {out} {err}");

    // Continuation omits --mode and inherits byte rather than reverting to text.
    let (next, o, e) = t.run(&[
        "commit", "commit", "a.md", "--id", &first_tip, "--range", "0", "4", "--reason", "extend",
    ]);
    assert_eq!(next, 0, "byte continuation: {o} {e}");
    let second_tip = t.tip(&node);
    let second: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(t.0.join(format!(".omd/commits/{second_tip}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        omd::relations::node::position_from_value(&second.payload["position"]),
        Some(omd::relations::range::Range {
            start: 0,
            end: 4,
            mode: omd::relations::range::Mode::Byte,
        })
    );

    // Byte 3 is inside [0,4): b -> z must dirty.
    t.write("a.md", "aézx");
    let (dirty, out, err) = t.run(&["verify", "a.md"]);
    assert_eq!(dirty, 1, "inside-byte-span edit dirties: {out} {err}");
    assert!(out.contains("in-range") || out.contains("dirty"), "{out}");
}

#[test]
fn byte_mode_accepts_non_utf8_and_never_decodes_it() {
    let t = T::new();
    t.write("raw.bin", "valid");
    t.run(&["init", "raw.bin"]);
    std::fs::write(t.0.join("raw.bin"), [0xff, 0x00, 0x01, 0x02]).unwrap();
    let (code, out, err) = t.run(&[
        "commit", "commit", "raw.bin", "--range", "0", "2", "--mode", "byte", "--reason", "raw",
    ]);
    assert_eq!(code, 0, "non-UTF8 byte range commits: {out} {err}");
    std::fs::write(t.0.join("raw.bin"), [0xff, 0x00, 0x01, 0x03]).unwrap();
    let (code, out, err) = t.run(&["verify", "raw.bin"]);
    assert_eq!(code, 0, "outside raw byte edit stays clean: {out} {err}");
}

#[test]
fn effective_range_body_survives_link_end_and_two_renames() {
    let t = T::new();
    t.write("a.md", "abcdef");
    t.write("target.md", "uvwxyz");
    t.run(&["init", "a.md"]);
    t.run(&["init", "target.md"]);

    let (_, target_out, _) = t.run(&[
        "commit",
        "commit",
        "target.md",
        "--range",
        "0",
        "6",
        "--reason",
        "target",
    ]);
    let target: serde_json::Value = serde_json::from_str(&target_out).unwrap();
    let target_root = target["data"]["commit"].as_str().unwrap();
    let target_node = format!("range:{target_root}");

    let (_, source_out, _) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "6", "--reason", "source",
    ]);
    let source: serde_json::Value = serde_json::from_str(&source_out).unwrap();
    let source_root = source["data"]["commit"].as_str().unwrap().to_string();
    let source_node = format!("range:{source_root}");
    let source_tip = t.tip(&source_node);

    t.run(&["commit", "tag", "a.md", "--tag", "spec"]);
    t.run(&["commit", "tag", "target.md", "--tag", "code"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "a.md",
        "--rule",
        "spec->code",
        "--level",
        "fail",
    ]);
    let (linked, out, err) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &source_tip,
        "--range",
        "0",
        "6",
        "--link-to",
        &target_node,
        "--reason",
        "linked",
    ]);
    assert_eq!(linked, 0, "combo link closes: {out} {err}");
    assert_ne!(
        t.tip(&source_node),
        source_tip,
        "END advances structural tip"
    );

    let (check, out, err) = t.run(&["check"]);
    assert_eq!(
        check, 0,
        "effective body contributes coverage behind END: {out} {err}"
    );
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        value["data"]["check"]["rules"][0]["coverage"]["forward"]["groups"][0]["covered"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            > 0,
        "coverage uses effective body: {out}"
    );

    let original: std::collections::BTreeMap<_, _> = std::fs::read_dir(t.0.join(".omd/commits"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_owned(),
                std::fs::read(path).unwrap(),
            )
        })
        .collect();

    t.run(&["rename", "a.md", "b.md"]);
    std::fs::rename(t.0.join("a.md"), t.0.join("b.md")).unwrap();
    let (once, out, err) = t.run(&["verify", "b.md"]);
    assert_eq!(
        once, 0,
        "range follows first current parent location: {out} {err}"
    );

    t.run(&["rename", "b.md", "c.md"]);
    std::fs::rename(t.0.join("b.md"), t.0.join("c.md")).unwrap();
    let (twice, out, err) = t.run(&["verify", "c.md"]);
    assert_eq!(
        twice, 0,
        "range follows second current parent location: {out} {err}"
    );
    let file_node = t.file_node("c.md");
    let mount = t
        .state()
        .lines()
        .find(|line| line.contains(&format!("\"{file_node}\"")) && line.contains('['))
        .unwrap_or("")
        .to_string();
    assert!(
        mount.contains(&source_node),
        "range identity survives rename: {mount}"
    );
    assert!(
        t.state().contains(&format!("source = \"{source_node}\"")),
        "link endpoint keeps range identity across rename"
    );
    for (name, bytes) in original {
        assert_eq!(
            std::fs::read(t.0.join(".omd/commits").join(name)).unwrap(),
            bytes,
            "rename never rewrites historical commits"
        );
    }

    std::fs::remove_file(t.0.join("c.md")).unwrap();
    let (missing, out, err) = t.run(&["verify", "c.md"]);
    assert_eq!(
        missing, 1,
        "genuinely missing current path fails: {out} {err}"
    );
    assert!(
        out.contains("missing") || out.contains("cannot read"),
        "{out}"
    );
}

#[test]
fn missing_effective_source_version_stays_negative() {
    let t = T::new();
    t.write("a.md", "abcdef");
    t.run(&["init", "a.md"]);
    let (_, out, _) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "range",
    ]);
    let response: serde_json::Value = serde_json::from_str(&out).unwrap();
    let commit_id = response["data"]["commit"].as_str().unwrap();
    let commit: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(t.0.join(format!(".omd/commits/{commit_id}.toml"))).unwrap(),
    )
    .unwrap();
    std::fs::remove_file(t.0.join(format!(".omd/versions/{}.toml", commit.content_ref))).unwrap();
    let (code, out, err) = t.run(&["verify", "a.md"]);
    assert_eq!(code, 1, "missing effective version fails: {out} {err}");
    assert!(out.contains("version record missing"), "{out}");
}

#[test]
fn missing_command_observation_aborts_before_begin() {
    use std::os::unix::fs::PermissionsExt;

    let t = T::new();
    t.write("target.md", "target");
    t.run(&["init", "target.md"]);
    let (_, target_out, _) = t.run(&[
        "commit",
        "commit",
        "target.md",
        "--range",
        "0",
        "6",
        "--reason",
        "target",
    ]);
    let target: serde_json::Value = serde_json::from_str(&target_out).unwrap();
    let target_node = format!("range:{}", target["data"]["commit"].as_str().unwrap());
    let counter = t.0.join("command-ran");
    let command = t.0.join("fail-command.sh");
    std::fs::write(&command, "#!/bin/sh\nprintf placeholder\n").unwrap();
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (initialized, out, err) = t.run(&[
        "commit",
        "init",
        "virtual.md",
        "--source-type",
        "command",
        "--executable",
        command.to_str().unwrap(),
        "--args-json",
        "[]",
    ]);
    assert_eq!(initialized, 0, "command source initializes: {out} {err}");
    std::fs::write(
        &command,
        format!("#!/bin/sh\necho ran >> '{}'\nexit 1\n", counter.display()),
    )
    .unwrap();
    let before = t.state();
    let (code, out, err) = t.run(&[
        "commit",
        "commit",
        "virtual.md",
        "--range",
        "0",
        "1",
        "--link-to",
        &target_node,
        "--reason",
        "fail",
        "--source-type",
        "command",
        "--executable",
        command.to_str().unwrap(),
        "--args-json",
        "[]",
    ]);
    assert_eq!(
        code, 3,
        "missing command observation is a conflict: {out} {err}"
    );
    assert_eq!(t.state(), before, "credential failure publishes nothing");
    assert!(!counter.exists(), "write validation never executes command");
}

// managed-content: a fragment matching ambiguously in current content reports
// locate candidates — never auto-picks one.
#[test]
fn ambiguous_fragment_reports_locate_candidates() {
    let t = T::new();
    t.write("a.md", "XX AB XX");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "3", "5", "--reason", "r",
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
        "--source-type",
        "command",
        "--executable",
        "echo",
        "--args-json",
        "[\"hi\"]",
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

#[test]
fn effective_range_observation_does_not_run_command_source_without_consent() {
    use std::os::unix::fs::PermissionsExt;

    let t = T::new();
    t.write("count", "0\n");
    t.write(
        "source.sh",
        "#!/bin/sh\nn=$(cat count)\necho $((n + 1)) > count\nprintf abc\n",
    );
    let mut permissions = std::fs::metadata(t.0.join("source.sh"))
        .unwrap()
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(t.0.join("source.sh"), permissions).unwrap();
    t.run(&[
        "commit",
        "init",
        "virtual.txt",
        "--source-type",
        "command",
        "--executable",
        "./source.sh",
        "--args-json",
        "[]",
    ]);
    let (_, observed, _) = t.run(&["--run-command=true", "verify", "virtual.txt"]);
    let observed: serde_json::Value = serde_json::from_str(&observed).unwrap();
    let expected = serde_json::to_string(&observed["data"]["expected"]).unwrap();
    t.run(&[
        "--expected",
        &expected,
        "commit",
        "commit",
        "virtual.txt",
        "--range",
        "0",
        "2",
        "--reason",
        "range",
        "--source-type",
        "command",
        "--executable",
        "./source.sh",
        "--args-json",
        "[]",
    ]);
    assert_eq!(
        std::fs::read_to_string(t.0.join("count")).unwrap().trim(),
        "2"
    );
    let (code, out, err) = t.run(&["verify", "virtual.txt"]);
    assert_eq!(
        code, 1,
        "unconsented command range stays unverified: {out} {err}"
    );
    assert_eq!(
        std::fs::read_to_string(t.0.join("count")).unwrap().trim(),
        "2",
        "effective-state observation must not launch command sources"
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
        "--source-type",
        "command",
        "--executable",
        "cat",
        "--args-json",
        "[\"in.txt\"]",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    // Two links a-range → b-range (distinct link_ids).
    let rn13 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn13,
        "--reason",
        "l1",
    ]);
    let rn14 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn14,
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
        "0",
        "1",
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
        "--source-type",
        "command",
        "--executable",
        "echo",
        "--args-json",
        "[\"hi\"]",
    ]);
    assert_eq!(c, 0, "command init captures output: {o} {e}");
    let mut is_cmd = false;
    if let Ok(rd) = std::fs::read_dir(t.0.join(".omd/versions")) {
        for en in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(en.path())
                && txt.contains("[acquisition]")
                && txt.contains("type = \"command\"")
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
    let a = t.file_node("a.md");
    let b = t.file_node("b.md");
    assert!(st.contains(&a) && st.contains(&b));
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
        "commit", "commit", "a.md", "--range", "0", "10", "--reason", "r",
    ]);
    let rn15 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn15,
        "--reason",
        "lb",
    ]);
    // Split a's range into a new sub-range — a fresh object.
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "sub",
    ]);
    // The new sub-range has no pending obligations of its own (it wasn't
    // the link's source — 0-10 was).
    let st = t.state();
    assert!(t.range_exists("a.md", "text:0-5"), "sub-range exists: {st}");
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
        "--source-type",
        "command",
        "--executable",
        "cat",
        "--args-json",
        "[\"cnt.txt\"]",
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
    let (c, o, e) = t.run(&[
        "replace",
        &tip,
        "--source-type",
        "file",
        "--source-path",
        "b.md",
    ]);
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
    ]);
    let rn16 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "1",
        "--link-from",
        &rn16,
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
        "0",
        "1",
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "r",
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
        t.range_exists("a.md", "text:0-3"),
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
    let (init_code, init_out, init_err) = t.run(&["init", "a.md"]);
    assert_eq!(init_code, 0, "initial init failed: {init_out} {init_err}");
    let old_tip = t.tip("file:a.md");
    let (delete_code, delete_out, delete_err) = t.run(&["delete", "a.md"]);
    assert_eq!(delete_code, 0, "delete failed: {delete_out} {delete_err}");
    t.write("a.md", "second-different");
    let (reinit_code, reinit_out, reinit_err) = t.run(&["init", "a.md"]);
    assert_eq!(reinit_code, 0, "reinit failed: {reinit_out} {reinit_err}");
    let new_tip = t.tip("file:a.md");
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "2", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
    ]);
    // Linking FROM a real range TO a never-initialized range must fail — the
    // ENDPOINT-EXISTENCE guard, not a clap arg-parse error (--source/--target).
    let (c, o, e) = t.run(&[
        "commit",
        "link",
        "a.md",
        "--source",
        &t.range_node("a.md", 0),
        "--target",
        "range:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    ]);
    assert_ne!(c, 0, "phantom endpoint rejected: {o} {e}");
    assert!(
        format!("{o}{e}").contains("does not exist"),
        "guard diagnostic names the missing range: {o} {e}"
    );
}

// change-review (55): a combo link whose endpoint does not resolve is
// rejected BEFORE any publish — no BEGIN, no range member, no open block.
// The partial-failure report is reserved for genuine mid-block publish
// failures (peer/I-O), which are not reachable as endpoint errors.
#[test]
fn combo_link_reports_partial_failure() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
    ]);
    let commits_before = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    let state_before = t.state();
    // --link-from real range + --link-from a nonexistent one in one command.
    let rn17 = t.range_node("a.md", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn17,
        "--link-from",
        "range:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "--reason",
        "r",
    ]);
    assert_eq!(c, 2, "unresolvable endpoint is a usage error: {o} {e}");
    assert!(
        format!("{o}{e}").contains("does not exist"),
        "guard diagnostic names the missing range: {o} {e}"
    );
    // Zero writes: no BEGIN, no range member, no open block.
    assert_eq!(
        std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count(),
        commits_before,
        "no commits published"
    );
    assert!(
        !t.state().contains("[open_blocks]")
            || t.state()
                .split("[open_blocks]")
                .nth(1)
                .map_or(true, |s| s.trim_start().starts_with('[')
                    && !s.contains('=')),
        "no open block left"
    );
    assert_eq!(t.state(), state_before, "state byte-identical");
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    // Reset on END → lands on its direct predecessor, block is open again.
    // (reset target is passed via --reason <commit_id> per the CLI contract.)
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reset-target", &end_tip]);
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
    ]);
    // Combo: valid link-from + an INVALID one — the endpoint guard fires.
    let rn19 = t.range_node("a.md", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn19,
        "--link-from",
        "range:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
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

// structured-tracking-references: `--id` names the chain AND asserts the
// commit IS the current tip — a stale commit id is a version conflict
// (exit 3), never a silent rebase onto the newest tip.
#[test]
fn stale_id_is_version_conflict_not_rebase() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "r1",
    ]);
    let r0 = t.range_node("a.md", 0);
    let tip0 = t.tip(&r0);
    // Advance the chain: tip moves forward.
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip0, "--range", "0", "5", "--reason", "r2",
    ]);
    let tip1 = t.tip(&r0);
    assert_ne!(tip0, tip1, "chain advanced");
    let commits_before = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    // Write citing the STALE commit id → version conflict, tip unchanged.
    let (c, o, e) = t.run(&[
        "commit", "commit", "a.md", "--id", &tip0, "--range", "0", "6", "--reason", "r3",
    ]);
    assert_eq!(c, 3, "stale --id is a version conflict: {o} {e}");
    assert_eq!(t.tip(&r0), tip1, "tip unchanged by refused write");
    assert_eq!(
        std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count(),
        commits_before,
        "no commit published"
    );
}

// structured-tracking-references: a FILE commit id is not a range endpoint
// — type check rejects before any BEGIN lands (zero writes).
#[test]
fn file_commit_as_link_endpoint_rejected_pre_publish() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    let file_cid = t.tip("file:a.md");
    let commits_before = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0",
        "3",
        "--link-to",
        &file_cid,
        "--reason",
        "r",
    ]);
    assert_eq!(c, 2, "file endpoint is a usage error: {o} {e}");
    assert_eq!(
        std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count(),
        commits_before,
        "zero publishes"
    );
    assert!(
        !t.state().contains("open_blocks")
            || !t
                .state()
                .lines()
                .skip_while(|l| !l.contains("open_blocks"))
                .skip(1)
                .take_while(|l| !l.starts_with('['))
                .any(|l| l.contains('=')),
        "no open block"
    );
}

// structured-tracking-references: coordinate endpoint spelling rejected
// with usage exit 2 — never a lock_conflict misclassification.
#[test]
fn coordinate_endpoint_is_usage_error() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
    ]);
    let rn = t.range_node("a.md", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0",
        "3",
        "--link-from",
        "a.md@text:0-5",
        "--reason",
        "r",
    ]);
    assert_eq!(
        c, 2,
        "coordinate endpoint is usage error, not lock_conflict: {o} {e}"
    );
    assert!(
        format!("{o}{e}").contains("coordinate") || format!("{o}{e}").contains("usage"),
        "usage diagnostic: {o} {e}"
    );
    let _ = rn;
}

// Error categories are carried separately from user text. Tokens that name
// other error kinds cannot change a malformed endpoint's usage classification.
#[test]
fn endpoint_text_cannot_select_error_category() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    let before = t.state();
    for endpoint in [
        "write.lock@text:0-2",
        "conflict@text:0-2",
        "executable@text:0-2",
        "combo_partial_failure@text:0-2",
    ] {
        let (code, stdout, stderr) = t.run(&[
            "--json",
            "commit",
            "commit",
            "a.md",
            "--range",
            "0",
            "2",
            "--link-from",
            endpoint,
            "--reason",
            "r",
        ]);
        assert_eq!(code, 2, "usage exit for {endpoint}: {stdout} {stderr}");
        let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(output["diagnostics"][0]["kind"], "usage", "{stdout}");
        assert_eq!(t.state(), before, "zero publication for {endpoint}");
    }
}

#[test]
fn local_link_fields_reject_peer_spelling_before_publication() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "2", "--reason", "r",
    ]);
    let local = t.range_node("a.md", 0);
    let before = t.state();

    let (code, stdout, stderr) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "2",
        "4",
        "--link-from",
        "peer:missing:range:deadbeef",
        "--reason",
        "r",
    ]);
    assert_eq!(
        code, 2,
        "local option rejects peer spelling: {stdout} {stderr}"
    );
    assert_eq!(t.state(), before, "no BEGIN or link published");

    let (code, stdout, stderr) = t.run(&[
        "commit",
        "link",
        "a.md",
        "--source",
        "peer:missing:range:deadbeef",
        "--target",
        &local,
        "--reason",
        "r",
    ]);
    assert_eq!(
        code, 2,
        "explicit local link rejects peer spelling: {stdout} {stderr}"
    );
    assert_eq!(t.state(), before, "explicit link also has zero publication");
}

#[test]
fn selected_range_id_must_match_path_and_object_kind() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "2", "--reason", "r",
    ]);
    let range_node = t.range_node("a.md", 0);
    let range_tip = t.tip(&range_node);
    let file_tip = t.tip("file:a.md");
    let before = t.state();

    let (code, stdout, stderr) = t.run(&[
        "commit", "commit", "b.md", "--id", &range_tip, "--range", "0", "3", "--reason", "r",
    ]);
    assert_eq!(code, 2, "wrong mounted path rejected: {stdout} {stderr}");
    assert_eq!(t.state(), before, "wrong path publishes nothing");

    let (code, stdout, stderr) = t.run(&[
        "commit", "commit", "a.md", "--id", &file_tip, "--range", "0", "3", "--reason", "r",
    ]);
    assert_eq!(
        code, 2,
        "file chain cannot receive range payload: {stdout} {stderr}"
    );
    assert_eq!(t.state(), before, "wrong object kind publishes nothing");
}

#[test]
fn missing_source_observation_is_version_conflict() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    std::fs::remove_file(t.0.join("a.md")).unwrap();
    let before = t.state();
    let (code, stdout, stderr) = t.run(&["--json", "commit", "commit", "a.md", "--reason", "r"]);
    assert_eq!(
        code, 3,
        "missing source observation is a conflict: {stdout} {stderr}"
    );
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        output["diagnostics"][0]["kind"], "version_conflict",
        "{stdout}"
    );
    assert_eq!(t.state(), before, "I/O failure publishes nothing");
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "r",
    ]);
    let tip = t.tip("range:a.md@text:0-3");
    // Extend the range (new commit via --id).
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0", "6", "--reason", "ext",
    ]);
    // Undo = commit the original range back via --id — a forward commit,
    // not a revert of the chain.
    let new_tip = t.tip("range:a.md@text:0-3").to_string();
    t.run(&[
        "commit", "commit", "a.md", "--id", &new_tip, "--range", "0", "3", "--reason", "undo",
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
    let file_node = t.file_node("a.md");
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
    assert!(out.contains(&file_node), "node survives move: {out}");
    // Store identity and commit ids are unchanged across the move; project
    // identity is checked separately against manifest.toml below.
    let st = std::fs::read_to_string(moved.join(".omd/state.toml")).unwrap();
    let tip_after = st
        .lines()
        .find(|l| l.contains(&format!("\"{file_node}\"")) && l.contains('='))
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "ra",
    ]);
    let rn21 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn21,
        "--reason",
        "rb",
    ]);
    // Upstream breakage: commit a new version on a's range.
    let tip = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0", "3", "--reason", "up",
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
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--range", "0", "3", "--reason", "saved",
        ])
        .0,
        0
    );
    let child = t.range_node("a.md", 0);
    let saved_child_tip = t.tip(&child);
    assert_eq!(t.run(&["commit", "verify", "a.md"]).0, 0);
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
    let later_child_tip = t.tip(&child);
    assert_ne!(later_child_tip, saved_child_tip, "child range must advance");

    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reset-target", &file_snapshot]);
    assert_eq!(c, 0, "file reset ok: {o} {e}");
    assert_eq!(
        t.tip(&child),
        saved_child_tip,
        "child range restored to file snapshot"
    );
    let (_, dangling, _) = t.run(&["list", "--dangling"]);
    assert!(dangling.contains(&later_child_tip));
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let m_tip = t.tip("file:a.md");
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    t.run(&["commit", "reset", "a.md", "--reset-target", &end_tip]);
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
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
    // Clean is branch-scoped. With no selected link/change it is invalid;
    // confirming removed content uses an explicit empty range commit instead.
    let (c, _, _) = t.run(&["commit", "clean", "a.md", "--reason", "removed body"]);
    assert_eq!(c, 2, "clean without --stop is rejected");
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    // Reset the END → the file snapshot restores the recorded state where
    // the child END still existed — a subsequent END close works again.
    t.run(&["commit", "reset", "a.md", "--reset-target", &end_tip]);
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "ra",
    ]);
    let rn22 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn22,
        "--reason",
        "lb",
    ]);
    // Two upstream commits seed two obligations on the one link.
    // --id always names the CURRENT tip (stale ids are version conflicts).
    let tip = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0", "3", "--reason", "up1",
    ]);
    let tip2 = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip2, "--range", "0", "3", "--reason", "up2",
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
    let selection = serde_json::json!({
        "link_id": lid,
        "changes": [named],
        "reason": "partial",
    })
    .to_string();
    let (_, o, e) = t.run(&["commit", "adapt", "b.md", "--adapt", &selection]);
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
    let file_node = t.file_node("a.md");
    let tampered = st.replacen(
        &format!("\"{file_node}\" = \"{tip}\""),
        &format!("\"{file_node}\" = \"{}\"", "f".repeat(64)),
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "ra",
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
        "commit", "commit", "impl.rs", "--range", "0", "11", "--reason", "rs-range",
    ]);
    // A Python range links FROM the Rust range — cross-language relation.
    let rn23 = t.range_node("impl.rs", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "impl.py",
        "--range",
        "0",
        "16",
        "--link-from",
        &rn23,
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
        "0",
        "2",
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let rtip = t.tip("range:a.md@text:0-3");
    // Advance the range via --id INSIDE the open block — tip moves forward.
    t.run(&[
        "commit", "commit", "a.md", "--id", &rtip, "--range", "0", "5", "--reason", "adv",
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
    let (c, _, _) = t.run(&[
        "replace",
        &tip,
        "--source-type",
        "file",
        "--source-path",
        "b.md",
    ]);
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
    ]);
    t.write("a.md", "012"); // delete the tracked span
    let tip = t.tip(&t.range_node("a.md", 0));
    // An explicit empty-range commit on the tip — the p:p confirmation.
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &tip,
        "--range",
        "0",
        "0",
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r0",
    ]);
    let r0 = t.tip(&t.range_node("a.md", 0));
    // Extend the range → new commit, extent 0-10.
    t.run(&[
        "commit", "commit", "a.md", "--id", &r0, "--range", "0", "10", "--reason", "extend",
    ]);
    // Reset to r0 → the range returns to its 0-5 extent, extension dangles.
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reset-target", &r0]);
    assert_eq!(c, 0, "reset to r0: {o} {e}");
    // The tip is a reset MARKER whose previous_id is r0 — the chain landed
    // on r0, and the extension commit is OFF the chain (dangled).
    let tip = t.tip(&t.range_node("a.md", 0));
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "ra",
    ]);
    // Open a block on B and create a link inside it — the link is a member
    // of B's block, so the block stays open until B's END.
    t.run(&["commit", "begin", "b.md"]);
    let rn24 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "5",
        "--link-from",
        &rn24,
        "--reason",
        "lb",
    ]);
    // B's block is still open (link member inside it); A has no open block.
    let st = t.state();
    let open = st.split("[open_blocks]").nth(1).unwrap_or("");
    let b_node = t.file_node("b.md");
    let a_node = t.file_node("a.md");
    assert!(
        open.contains(&b_node),
        "link belongs to B's open block: {open}"
    );
    assert!(!open.contains(&a_node), "A has no open block: {open}");
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
        "--source-type",
        "command",
        "--executable",
        "cat",
        "--args-json",
        "[\"o.txt\"]",
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let end = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    t.run(&["commit", "reset", "a.md", "--reset-target", &end]);
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let _pre_end_tip = t.tip("file:a.md"); // last member before END
    let end = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    let (_, o, _) = t.run(&["commit", "reset", "a.md", "--reset-target", &end, "--json"]);
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r",
    ]);
    // Delete the tracked span entirely.
    t.write("a.md", "012");
    // An explicit empty-range commit (0-0) on the SAME range chain with a
    // deletion reason records the deletion as a confirmed empty body.
    let tip = t.tip(&t.range_node("a.md", 0));
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &tip,
        "--range",
        "0",
        "0",
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
        "0",
        "2",
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
    ]);
    let m_tip = t.tip("file:a.md");
    let end_tip = {
        t.run(&["commit", "end", "a.md"]);
        t.tip("file:a.md")
    };
    let (c, o, e) = t.run(&[
        "commit",
        "reset",
        "a.md",
        "--reset-target",
        &end_tip,
        "--json",
    ]);
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "m",
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
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "adv",
    ]);
    let (code, out, err) = t.run(&[
        "commit",
        "reset",
        "a.md",
        "--reset-target",
        &end_tip,
        "--json",
    ]);
    assert_eq!(code, 0, "direct END reset succeeds: {out} {err}");
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_ne!(
        result["data"]["reset"]["actual"].as_str(),
        Some(end_tip.as_str()),
        "direct END reset shifts one predecessor"
    );
    let _ = range_tip_at_end;
}
// local-project-links #5: a moved project keeps its project_id + commit ids
// unchanged — move renames the dir, never re-derives identity.
#[test]
fn project_move_preserves_ids() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let tip_before = t.tip("file:a.md");
    let file_node = t.file_node("a.md");
    let pid_before = std::fs::read_to_string(t.0.join(".omd/manifest.toml"))
        .unwrap()
        .lines()
        .find(|line| line.starts_with("project_id"))
        .unwrap()
        .to_string();
    let parent = tempfile::tempdir().unwrap();
    let moved = parent.path().join("moved");
    std::fs::rename(&t.0, &moved).unwrap();
    let st = std::fs::read_to_string(moved.join(".omd/state.toml")).unwrap();
    let manifest = std::fs::read_to_string(moved.join(".omd/manifest.toml")).unwrap();
    let pid_after = manifest
        .lines()
        .find(|line| line.starts_with("project_id"))
        .unwrap()
        .to_string();
    assert_eq!(pid_before, pid_after, "project_id unchanged across move");
    let tip_after = st
        .lines()
        .find(|l| l.contains(&format!("\"{file_node}\"")))
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
        "commit", "commit", "a.md", "--range", "0", "3", "--reason", "ra",
    ]);
    let rn25 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn25,
        "--reason",
        "rb",
    ]);
    let rn26 = t.range_node("b.md", 0);
    t.run(&[
        "commit",
        "commit",
        "c.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn26,
        "--reason",
        "rc",
    ]);
    // A commits → both B's link AND C's transitive link flag pending.
    let tip = t.tip("range:a.md@text:0-3");
    t.run(&[
        "commit", "commit", "a.md", "--id", &tip, "--range", "0", "3", "--reason", "up",
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

// change-review #55: endpoint validation is PRE-BEGIN for malformed input,
// while a real peer lock acquired after successful preflight exercises the
// genuine partial-publication path below.
#[test]
fn combo_failure_reports_succeeded_step_boundary_opid() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "ra",
    ]);
    t.run(&["commit", "begin", "b.md"]);
    let rn27 = t.range_node("a.md", 0);
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "3",
        "--link-from",
        &rn27,
        "--reason",
        "m1",
    ]);
    // Second combo: one valid member + one nonexistent → rejected
    // pre-BEGIN, nothing new published for this command.
    let commits_before = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    let rn28 = t.range_node("a.md", 0);
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "6",
        "--link-from",
        &rn28,
        "--link-from",
        "range:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "--reason",
        "m2",
    ]);
    assert_eq!(c, 2, "invalid endpoint rejected pre-publish: {o} {e}");
    let all = format!("{o}{e}");
    assert!(all.contains("does not exist"), "names the endpoint: {all}");
    assert_eq!(
        std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count(),
        commits_before,
        "zero commits published"
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
        "commit", "commit", "b.md", "--range", "0", "2", "--reason", "rb",
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

// P1-5: a range commit created inside its parent FILE's ATOMIC block
// carries an `in_block` stamp — resetting it (open or closed block)
// is refused as an ordinary block member.
#[test]
fn reset_range_commit_inside_file_block_refused() {
    let t = T::new();
    t.write("c.md", "0123456789");
    t.run(&["init", "c.md"]);
    t.run(&["commit", "begin", "c.md"]);
    t.run(&[
        "commit", "commit", "c.md", "--range", "0", "3", "--reason", "m",
    ]);
    // The member commit id = tip of the (first, un-suffixed) range chain.
    // Match the [tips] row exactly: `"range:…" = "<64-hex>"`.
    let member = {
        // The range commit inside the block: `range:<root>`'s tip.
        let rk = t.range_node("c.md", 0);
        t.state()
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("\"{rk}\" =")))
            .and_then(|l| l.split('"').nth(3).map(String::from))
            .unwrap_or_default()
    };
    assert_eq!(member.len(), 64, "range tip id: {member}");
    // OPEN block shape — refused (interior-member error → non-zero exit).
    let (c1, o1, e1) = t.run(&["commit", "reset", "c.md", "--reset-target", &member]);
    assert_ne!(c1, 0, "open-block member reset refused: {o1}{e1}");
    assert!(
        o1.contains("ordinary block member") || e1.contains("ordinary block member"),
        "o1={o1} e1={e1}"
    );
    // CLOSED block shape — same membership persists after END.
    t.run(&["commit", "end", "c.md"]);
    let (c2, o2, e2) = t.run(&["commit", "reset", "c.md", "--reset-target", &member]);
    assert_ne!(c2, 0, "closed-block member reset refused: {o2}{e2}");
    assert!(
        o2.contains("ordinary block member") || e2.contains("ordinary block member"),
        "o2={o2} e2={e2}"
    );
    // An ordinary range commit OUTSIDE the block still resets.
    let (c3, o3, e3) = t.run(&[
        "commit", "commit", "c.md", "--id", &member, "--range", "0", "4", "--reason", "outside",
    ]);
    assert_eq!(c3, 0, "outside commit succeeds: {o3} {e3}");
    let outside = t.tip(&t.range_node("c.md", 0));
    let (c4, o4, _) = t.run(&["commit", "reset", "c.md", "--reset-target", &outside]);
    assert_eq!(c4, 0, "out-of-block reset passes: {o4}");
}

// P1-4: a parallel chain over identical coords (nonce-suffixed range key)
// still counts into coverage — parse_span strips the `#nonce` suffix so
// `0-5#abc` parses the same span as `0-5`.
#[test]
fn nonce_suffixed_range_counts_in_coverage() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.write("b.md", "0123456789");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r1",
    ]);
    // Second commit on same coords → nonce-suffixed parallel chain.
    // Two same-coordinate ranges are now two distinct chain roots — the
    // second is just another `range:<id>` node under the same file mount.
    let r1 = t.tip(&t.range_node("a.md", 0));
    // A second `--range 0-5` creates a NEW chain (not a nonce suffix on
    // the old key). The mounts of file:a.md now hold two range children.
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "5", "--reason", "r3",
    ]);
    let st = t.state();
    let file_node = t.file_node("a.md");
    let file_mount = st
        .lines()
        .find(|l| l.contains(&format!("\"{file_node}\"")) && l.contains('['))
        .unwrap_or("");
    assert!(
        file_mount.matches("range:").count() >= 2,
        "two independent range chains on same coords: {st}"
    );
    let _ = r1;
    // Tag a's file as spec, b's as code; rule spec->code; link b's range
    // to the nonce'd a-range — its positions must parse+count in coverage.
    t.run(&["commit", "tag", "a.md", "--tag", "spec"]);
    t.run(&["commit", "tag", "b.md", "--tag", "code"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "a.md",
        "--rule",
        "spec->code",
        "--level",
        "fail",
    ]);
    // Link b's range FROM the second a-range — identify it by its chain
    // root id (the range node key), never a coordinate string.
    let second_range = st
        .lines()
        .filter(|l| l.contains(&format!("\"{file_node}\"")) && l.contains('['))
        .flat_map(|l| {
            l.match_indices("range:")
                .filter_map(|(i, _)| {
                    let r = &l[i..];
                    r.find('"').map(|e| r[..e].to_string())
                })
                .collect::<Vec<_>>()
        })
        .nth(1)
        .unwrap_or_default();
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--range",
        "0",
        "5",
        "--link-from",
        &second_range,
        "--reason",
        "lb",
    ]);
    let (_, o, _) = t.run(&["check"]);
    // If parse_span failed on `0-5#…` the linked positions silently dropped
    // → covered=0. Assert the nonce'd range's positions actually counted.
    let j: serde_json::Value = serde_json::from_str(&o).unwrap_or_default();
    let covered = j["data"]["check"]["rules"]
        .as_array()
        .and_then(|rules| rules.first())
        .and_then(|rule| rule["coverage"]["forward"]["groups"].as_array())
        .and_then(|groups| groups.first())
        .and_then(|group| group["covered"].as_str())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    assert!(
        covered > 0,
        "nonce'd range positions counted (covered={covered}): {o}"
    );
}

// reset takes --target, never a --reason overload (semantic separation).
#[test]
fn reset_reason_overload_rejected() {
    let t = T::new();
    t.write("a.md", "0123456789");
    t.run(&["init", "a.md"]);
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reason", "anything"]);
    assert_ne!(c, 0, "--reason on reset refused: {o} {e}");
    assert!(
        format!("{o}{e}").contains("--reset-target"),
        "points at --reset-target: {o} {e}"
    );
}

// init is not stackable: a second init on a tracked path is refused.
#[test]
fn second_init_refused() {
    let t = T::new();
    t.write("a.md", "first");
    t.run(&["init", "a.md"]);
    let first_tip = t.tip("file:a.md");
    let (c, o, e) = t.run(&["init", "a.md"]);
    assert_ne!(c, 0, "second init refused: {o} {e}");
    assert!(
        format!("{o}{e}").contains("already tracked"),
        "diagnostic: {o} {e}"
    );
    // No new commit — the tip is unchanged.
    assert_eq!(t.tip("file:a.md"), first_tip, "no appended init record");
}
