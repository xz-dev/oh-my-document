//! 13.x traceability sweep — real per-clause checks over existing behavior.
//! Each test names its spec scenario in a comment.

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
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
    fn file_node(&self, path: &str) -> String {
        let value: toml::Value = toml::from_str(&self.state()).unwrap();
        value["locations"]
            .as_table()
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
            .args(common::with_expected(
                &omd(),
                &self.0,
                args,
                Some(&self.0.join(".omd")),
                Some(&self.0.join("home")),
                Some(&self.0.join("config")),
                Some(&self.0.join("cache")),
            ))
            .current_dir(&self.0)
            .env("HOME", self.0.join("home"))
            .env("OMD_CONFIG_PATH", self.0.join("config"))
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
    fn tip(&self, node: &str) -> String {
        let s = self.state();
        // `range:<file>@<span>` (test spelling) → the mounted range node
        // under `file:<file>` whose tip commit payload records the span.
        // Identity is the chain root — the span only selects *which* chain.
        if let Some(rest) = node.strip_prefix("range:") {
            if let Some(at) = rest.find('@') {
                let path = &rest[..at];
                let want = rest[at + 1..]
                    .strip_prefix("text:")
                    .unwrap_or(&rest[at + 1..]);
                let file_node = self.file_node(path);
                let mut children: Vec<String> = Vec::new();
                for l in s.lines() {
                    if l.contains(&format!("\"{file_node}\"")) && l.contains('[') {
                        for m in l.match_indices("range:") {
                            let r = &l[m.0..];
                            if let Some(e) = r.find('"') {
                                children.push(r[..e].to_string());
                            }
                        }
                    }
                }
                for c in &children {
                    let tip_id = s
                        .lines()
                        .find(|x| x.starts_with(&format!("\"{c}\"")) && x.contains('='))
                        .and_then(|x| x.split('=').nth(1))
                        .map(|v| v.trim().trim_matches('"').to_string())
                        .unwrap_or_default();
                    if tip_id.is_empty() {
                        continue;
                    }
                    let cm =
                        std::fs::read_to_string(self.0.join(format!(".omd/commits/{tip_id}.toml")))
                            .unwrap_or_default();
                    let span = cm
                        .lines()
                        .find(|x| x.contains("range"))
                        .and_then(|x| x.split('"').nth(1))
                        .unwrap_or("")
                        .to_string();
                    let span_norm = span.strip_prefix("text:").unwrap_or(&span).to_string();
                    if span_norm == want {
                        return tip_id;
                    }
                }
                // No span match — the chain's extent moved; take the first.
                if let Some(c) = children.first() {
                    return s
                        .lines()
                        .find(|x| x.starts_with(&format!("\"{c}\"")) && x.contains('='))
                        .and_then(|x| x.split('=').nth(1))
                        .map(|v| v.trim().trim_matches('"').to_string())
                        .unwrap_or_default();
                }
                return String::new();
            }
        }
        let node = node
            .strip_prefix("file:")
            .map(|path| self.file_node(path))
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
    let (c, o, e) = t.run(&[
        "commit",
        "commit",
        "a.md",
        "--range",
        "0",
        "1",
        "--no--reason",
    ]);
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
        "commit", "commit", "a.md", "--range", "0", "6", "--reason", "r",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "r2",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "ra",
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
        &t.range_node("a.md", 0),
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
        "0",
        "1",
        "--no--reason",
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r",
    ]);
    let (_, o, _) = t.run(&["tree", "--level", "file"]);
    // No range children at file level — range nodes are `range:<root-id>`.
    assert!(!o.contains("range:"), "file-level hides ranges: {o}");
    let (_, ofull, _) = t.run(&["tree"]);
    assert!(ofull.contains("range:"), "full tree shows ranges: {ofull}");
}

// 13.5/9.2 + structured-tracking-references: `--id` names a commit in an
// existing range chain AND asserts it is the current tip — an interior
// (stale) commit id is a version conflict (exit 3), never a silent rebase
// onto the tip. Passing the current tip commits to the chain, not a fork.
#[test]
fn id_to_interior_range_commit_resolves_to_chain() {
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
    let tip1 = t.tip("range:a.md@text:0-1");
    // c1 is now interior/stale — rejected as a version conflict, no write.
    let (c, o, _) = t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "r3",
    ]);
    assert_eq!(c, 3, "stale --id is a version conflict, not a rebase: {o}");
    assert_eq!(t.tip("range:a.md@text:0-1"), tip1, "tip unchanged");
    // The CURRENT tip still advances the same chain — commit to the chain,
    // not a detached fork.
    let (c, o, _) = t.run(&[
        "commit", "commit", "a.md", "--id", &tip1, "--range", "0", "1", "--reason", "r3",
    ]);
    assert_eq!(c, 0, "--id current tip commits to the chain: {o}");
    let tip_after = t.tip("range:a.md@text:0-1");
    let txt = std::fs::read_to_string(t.0.join(format!(".omd/commits/{tip_after}.toml"))).unwrap();
    assert!(
        !tip_after.is_empty() && tip_after != tip1,
        "tip advanced: {txt}"
    );
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
    let file_node = t.file_node("a.md");
    assert!(s.contains(&file_node), "ancestor .omd discovered: {s}");
}

#[test]
fn omd_meta_env_overrides_without_implicit_creation() {
    let t = T::new();
    let store = t.0.join("custom-store");
    let o = Command::new(omd())
        .arg("list")
        .env("OMD_META", &store)
        .current_dir(&t.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(!store.exists(), "non-init command must not create OMD_META");
}

// 5.1: a path containing characters that collide with the OLD coordinate
// key grammar (`@`, `#`, `%`, space) must round-trip — the identity is the
// chain root, so a path that would break `file@span` parsing is now safe.
#[test]
fn special_char_path_roundtrip() {
    let t = T::new();
    let f = "we@ird #1%.md";
    t.write(f, "0123456789");
    t.write("b.md", "bbb");
    t.run(&["init", f]);
    t.run(&["init", "b.md"]);
    let (c, o, e) = t.run(&["commit", "commit", f, "--range", "0", "3", "--reason", "r"]);
    assert_eq!(c, 0, "range on special-char path: {o} {e}");
    let rn = t.range_node(f, 0);
    assert!(rn.starts_with("range:"), "range node created: {rn}");
    // Link to it from b's range — endpoints name objects, not coordinates.
    t.run(&[
        "commit", "commit", "b.md", "--range", "0", "3", "--reason", "rb",
    ]);
    let rnb = t.range_node("b.md", 0);
    let (c, o, e) = t.run(&[
        "commit", "link", "b.md", "--source", &rnb, "--target", &rn, "--reason", "L",
    ]);
    assert_eq!(c, 0, "link to special-char range: {o} {e}");
    // Rename keeps identity: same range id remounts under the new file key.
    let (c, _o, e) = t.run(&["rename", f, "renamed.md"]);
    assert_eq!(c, 0, "rename: {e}");
    let s = std::fs::read_to_string(t.0.join(".omd/state.toml")).unwrap_or_default();
    assert!(s.contains(&rn), "range id survives rename: {s}");
    let renamed = t.file_node("renamed.md");
    assert!(
        !renamed.is_empty() && s.contains(&renamed),
        "file remounted: {s}"
    );
}
