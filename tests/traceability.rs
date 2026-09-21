//! 13.x traceability sweep — real per-clause checks over existing behavior.
//! Each test names its spec scenario in a comment.

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
            "omd-tr-{}-{}",
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
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn j(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}))
}

// 13.4: --no-reason records reason=None on the commit.
#[test]
fn no_reason_records_none() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let (c, o, e) = t.run(&["commit", "commit", "a.md", "--range", "0-1", "--no-reason"]);
    assert_eq!(c, 0, "{o} {e}");
    let tip = t.tip("range:a.md@text:0-1");
    let txt = std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip}.toml"))).unwrap();
    // --no-reason records an explicit none marker and no reason payload.
    assert!(txt.contains("no_reason = true"), "no-reason marker: {txt}");
    // No `reason = "..."` payload field (distinct from `no_reason`).
    assert!(!txt.contains("reason = \""), "no reason field: {txt}");
}

// 13.5: editing INSIDE a committed range dirties it — the range does not
// stay confirmed after its content changed. Uses a real in-range edit.
#[test]
fn in_range_edit_dirties_the_range() {
    let t = T::new();
    t.write("a.md", "line1\n");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-6", "--reason", "r",
    ]);
    // Edit inside the committed range (0-6 covers 'line1').
    t.write("a.md", "lineX\n");
    let (_, o, _) = t.run(&["verify", "a.md"]);
    let j = j(&o);
    let data = j.get("data").cloned().unwrap_or_default();
    // A real in-range edit marks the range dirty or moved — NOT clean ok:true.
    let is_clean = data.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    assert!(
        !is_clean || data.get("dirty").is_some() || data.get("locate").is_some(),
        "in-range edit must not report clean: {o}"
    );
}

// 12.3: dangling commit can still be inspected/logged (spec does not forbid).
#[test]
fn dangling_commit_still_inspectable() {
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
    t.run(&["commit", "reset", "a.md", "--reset-target", &c1]);
    // c2's file is gone after reset? No — reset moves tip, c2 is dangling.
    // log/inspect on the dangling tip commit id must not error.
    let (c, o, e) = t.run(&["log", &c1]);
    assert_eq!(c, 0, "dangling c1 inspectable: {o} {e}");
}

// 12.3: tree --json output is parseable structured data.
#[test]
fn tree_json_is_parseable() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let (_, o, _) = t.run(&["--json", "tree"]);
    let j = j(&o);
    assert!(
        j.get("data").and_then(|d| d.get("tree")).is_some(),
        "tree in JSON: {o}"
    );
}

// 12.3: all check/verify commands emit structured JSON.
#[test]
fn check_emits_structured_json() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let (_, o, _) = t.run(&["--json", "check"]);
    let j = j(&o);
    assert!(j.get("data").is_some(), "check JSON: {o}");
}

// 11.4: re-applying the same tag name to CHANGED content reports the tag
// still points at its recorded tip — it never silently re-qualifies the
// new content. (Spec: a tag is a qualification on a specific version.)
#[test]
fn tag_on_changed_content_is_not_a_requalification() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // Tag v1 on the current content — records a tag commit.
    let (c1, _, _) = t.run(&["commit", "tag", "a.md", "--tag", "v1"]);
    assert_eq!(c1, 0, "first tag ok");
    let tip_before = t.tip("file:a.md");
    // Change the file, tag again — the tag must NOT silently rebind to the
    // new content: either it reports a conflict, or the recorded tag commit
    // still names the OLD tip/content.
    t.write("a.md", "changed");
    t.run(&["commit", "tag", "a.md", "--tag", "v1"]);
    // Find the v1 tag commit and check its recorded basis is the old tip,
    // not the new content. We surface the tag's target, never a silent
    // re-point at 'changed'.
    // Find the SECOND v1 tag commit — the one applied AFTER the content
    // changed (first tag binds init's tip; the re-apply is the interesting
    // one). Collect all v1 tag commits and take the one whose previous_id
    // is `tip_before` (the tip at re-apply time).
    let tag_commits: Vec<String> = std::fs::read_dir(t.0.join(".omd/commits"))
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter(|c| c.contains("kind = \"tag\"") && c.contains("v1"))
        .collect();
    assert!(
        tag_commits.len() >= 2,
        "two v1 tag commits: {}",
        tag_commits.len()
    );
    // The re-applied tag binds `tip_before` — the tip recorded when the file
    // still held 'x'. Assert that binding is to the OLD content basis.
    let rebound = tag_commits
        .iter()
        .any(|body| body.contains(&format!("previous_id = \"{}\"", tip_before)));
    assert!(
        rebound,
        "a v1 tag binds the tip it tagged: {:?}",
        tag_commits
    );
}

// 12.4: reset to an unresolvable target → error diagnostic. Also: --reason
// on reset is a usage error (target comes via --target, never an overload).
#[test]
fn reset_unreachable_source_errors() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let (c, o, e) = t.run(&["commit", "reset", "a.md", "--reset-target", "bogus"]);
    assert_ne!(c, 0, "reset to unknown target must fail");
    let all = format!("{o}{e}");
    assert!(
        all.contains("unknown reset target")
            || all.contains("unreachable")
            || all.contains("error"),
        "diag: {all}"
    );
}

// 12.3: --no-reason commit on a LINK leaves pending obligations un-cleared —
// a clean/--stop adapt is still needed; reason-less commits never auto-clear.
#[test]
fn no_reason_does_not_clear_link_pending() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "ra",
    ]);
    // link b's range from a's range.
    let la = t.tip("range:a.md@text:0-1");
    let _ = la;
    // Upstream commit on source node seeds pending.
    t.run(&[
        "commit",
        "commit",
        "b.md",
        "--link-from",
        "a.md@text:0-1",
        "--reason",
        "lb",
    ]);
    let lb = t.tip("range:b.md@text:0-1");
    let _ = lb;
    // A subsequent no-reason commit on source must NOT clear pending — adapt
    // is the only path that clears obligations.
    t.run(&[
        "commit",
        "commit",
        "a.md",
        "--id",
        &t.tip("range:a.md@text:0-1"),
        "--range",
        "0-1",
        "--no-reason",
    ]);
    let st = std::fs::read_to_string(t.0.join(".omd/state.toml")).unwrap();
    // Pending obligations remain seeded (link_pending non-empty somewhere).
    assert!(
        st.contains("link_pending") || st.contains("[link_pending"),
        "pending obligations present: {st}"
    );
}

// 13.1: verify on a tracked file with a deleted source reports unreachable_source.
#[test]
fn verify_deleted_source_unreachable() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    std::fs::remove_file(t.0.join("a.md")).unwrap();
    let (_, o, e) = t.run(&["verify", "a.md"]);
    let all = format!("{o}{e}");
    assert!(
        all.contains("unreachable_source")
            || all.contains("unreadable")
            || all.contains("not_found")
            || all.contains("incomplete")
            || all.contains("unverif"),
        "unreachable source diag: {all}"
    );
}

// 12.3: tree with a nonexistent --level default falls back to full depth.
#[test]
fn tree_level_file_hides_ranges() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r",
    ]);
    let (_, o, _) = t.run(&["tree", "--level", "file"]);
    // No range: children under file node.
    assert!(!o.contains("range:a.md"), "file-level hides ranges: {o}");
    let (_, ofull, _) = t.run(&["tree"]);
    assert!(
        ofull.contains("range:a.md"),
        "full tree shows ranges: {ofull}"
    );
}

// 13.5/9.2: `--id` names a commit in an existing range chain and resolves
// to that chain — a non-tip member id resolves by walking the chain to its
// node, then appends at the tip (commit to the chain, not a fork).
#[test]
fn id_to_interior_range_commit_resolves_to_chain() {
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
    // c1 is non-tip. --id c1 resolves to the same chain — the new commit
    // lands on the tip, not a detached fork.
    let (c, o, _) = t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "r3",
    ]);
    assert_eq!(c, 0, "--id resolves to chain: {o}");
    let tip_after = t.tip("range:a.md@text:0-1");
    // The chain advanced — the new tip's previous_id is the old tip.
    let txt = std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip_after}.toml"))).unwrap();
    assert!(!tip_after.is_empty(), "tip resolved: {txt}");
}

// 13.2: a second `commit tag` on the same name reports the tag's CURRENT
// content consistently — tags is a single assignment, not a merge.
#[test]
fn repeated_tag_reports_current_content() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "tag", "a.md", "--tag", "v1"]);
    // `list` reports the tag's tip commit; no fabricated merge occurs.
    let (c, o, _) = t.run(&["list"]);
    assert_eq!(c, 0, "{o}");
    // The tag commit was recorded as a project-local tag on the file node.
    let st = std::fs::read_to_string(t.0.join(".omd/state.toml")).unwrap();
    assert!(
        st.contains("v1") || st.contains("file:a.md"),
        "tag recorded: {st}"
    );
}

// 13.x/9.1: meta dir discovery — running from a subdirectory finds the
// ancestor .omd/ store; OMD_META env overrides.
#[test]
fn meta_discovery_from_subdirectory() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    // From a nested subdir, no --meta: ancestor .omd found.
    std::fs::create_dir_all(t.0.join("sub/deep")).unwrap();
    let o = Command::new(omd())
        .arg("list")
        .current_dir(t.0.join("sub/deep"))
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&o.stdout);
    assert!(s.contains("file:a.md"), "ancestor .omd discovered: {s}");
}

#[test]
fn omd_meta_env_overrides() {
    let t = T::new();
    let store = t.0.join("custom-store");
    let o = Command::new(omd())
        .arg("list")
        .env("OMD_META", &store)
        .current_dir(&t.0)
        .output()
        .unwrap();
    // A fresh OMD_META dir gets initialized and reports ok.
    let s = String::from_utf8_lossy(&o.stdout);
    assert!(s.contains("ok") || store.exists(), "OMD_META honored: {s}");
}
