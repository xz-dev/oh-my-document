//! `omd audit` lifecycle through the real binary: add → colored show →
//! conclusion patches → filters; journal endpoints as link targets;
//! reset-withdrawn audit endpoint judged withdrawn; broken link exit 1.

mod common;
use std::process::Command;

static UNIQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
            "omd-audit-{}-{}",
            std::process::id(),
            UNIQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    fn json(out: &str) -> serde_json::Value {
        serde_json::from_str(out).expect("stdout is JSON")
    }
    fn write(&self, p: &str, c: &str) {
        let f = self.0.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, c).unwrap();
    }
    fn ranges_of(&self, file: &str) -> Vec<String> {
        let (_, out, _) = self.run(&["tree", "--json"]);
        T::json(&out)["data"]["tree"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["path"].as_str() == Some(file))
            .flat_map(|f| f["children"].as_array().cloned().unwrap_or_default())
            .map(|r| r["node"].as_str().unwrap_or_default().to_string())
            .collect()
    }
    /// Tip commit of `file:<key>` for file `path`.
    fn tip_of(&self, path: &str) -> String {
        let (_, out, _) = self.run(&["tree", "--json"]);
        T::json(&out)["data"]["tree"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"].as_str() == Some(path))
            .map(|f| f["node"].as_str().unwrap_or_default().to_string())
            .unwrap_or_default()
    }
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Build a file+range, an audit seeded on the file tip, return (range, audit_id).
fn setup(t: &T) -> (String, String) {
    t.write("a.md", "aaaa\n");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0", "4"]);
    let tip = t.tip_of("a.md");
    let (c, out, err) = t.run(&["audit", "add", &tip, "--text", "review this"]);
    assert_eq!(c, 0, "{out} {err}");
    let aid = T::json(&out)["data"]["audit"].as_str().unwrap().to_string();
    (t.ranges_of("a.md").remove(0), aid)
}

#[test]
fn audit_lifecycle_add_list_show_conclusions() {
    let t = T::new();
    let (_ra, aid) = setup(&t);

    // list --status pending finds the fresh audit.
    let (c, out, _) = t.run(&["audit", "list", "--status", "pending", "--json"]);
    assert_eq!(c, 0, "{out}");
    assert_eq!(T::json(&out)["data"]["total"], 1);

    // show walks the (empty) colored map without error.
    let (c, out, _) = t.run(&["audit", "show", &aid, "--json"]);
    assert_eq!(c, 0, "{out}");
    let d = &T::json(&out)["data"];
    assert_eq!(d["conclusion"], "pending");
    assert!(d["sections"]["downstream"].is_object());
    assert!(d["sections"]["upstream"].is_object());

    // fail appends a patch; command still exits 0 (fail is a record, not a
    // crash). --status fail hits, --status pending misses.
    let (c, out, err) = t.run(&["audit", "fail", &aid, "--text", "bad edge"]);
    assert_eq!(c, 0, "{out} {err}");
    let (c, out, _) = t.run(&["audit", "list", "--status", "fail", "--json"]);
    assert_eq!(T::json(&out)["data"]["total"], 1);
    let (_, out, _) = t.run(&["audit", "list", "--status", "pending", "--json"]);
    assert_eq!(T::json(&out)["data"]["total"], 0);
    let _ = c;

    // pass flips the conclusion again — append-only, never overwrite.
    t.run(&["audit", "pass", &aid, "--text", "actually fine"]);
    let (_, out, _) = t.run(&["audit", "show", &aid, "--json"]);
    assert_eq!(T::json(&out)["data"]["conclusion"], "pass");
}

#[test]
fn range_to_audit_link_is_healthy() {
    let t = T::new();
    let (ra, aid) = setup(&t);
    let (c, out, err) = t.run(&[
        "commit",
        "link",
        "a.md",
        "--source",
        &ra,
        "--target",
        &format!("audit:{aid}"),
        "--reason",
        "fix backlinks",
    ]);
    assert_eq!(c, 0, "{out} {err}");
    let (_, out, _) = t.run(&["links", "list", "--json"]);
    let items = T::json(&out)["data"]["items"].as_array().cloned().unwrap();
    assert!(items.iter().any(|l| l["status"] == "healthy"), "{items:?}");
}

#[test]
fn audit_show_wiki_refs_resolve_and_flag_dot_error() {
    let t = T::new();
    let (_ra, aid) = setup(&t);
    // One resolvable ref (self) + one dangling ref.
    t.run(&[
        "audit",
        "pass",
        &aid,
        "--text",
        &format!("wiki audit:{aid} resolves; audit:deadbeef00deadbeef does not"),
    ]);
    let (c, out, _) = t.run(&["audit", "show", &aid, "--json"]);
    assert_eq!(c, 0, "{out}");
    let refs = T::json(&out)["data"]["references"]
        .as_array()
        .cloned()
        .unwrap();
    let resolved = refs.iter().any(|r| r["status"] == "resolved");
    let doted = refs.iter().any(|r| r["status"] == "dot_error");
    assert!(resolved && doted, "{refs:?}");
}

#[test]
fn reset_withdraws_audit_patch_and_link_marks_withdrawn() {
    let t = T::new();
    let (ra, aid) = setup(&t);
    // Conclusion patch — then link the range to the audit's *tip* commit.
    t.run(&["audit", "fail", &aid, "--text", "found it"]);
    let (_, out, _) = t.run(&["audit", "list", "--json"]);
    let tip = T::json(&out)["data"]["items"][0]["tip"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(tip, aid, "patch moved the tip");
    let (c, out, err) = t.run(&[
        "commit",
        "link",
        "a.md",
        "--source",
        &ra,
        "--target",
        &tip, // pin the patch commit directly
        "--reason",
        "pins the fail commit",
    ]);
    assert_eq!(c, 0, "{out} {err}");
    // Reset the audit chain at the patch's PARENT (init) — withdraws the
    // patch itself. --reset-target names the commit to land on; commits
    // *after* it are withdrawn. To withdraw the patch, target the init.
    let (c, out, err) = t.run(&["commit", "reset", "a.md", "--reset-target", &aid]);
    assert_eq!(c, 0, "{out} {err}");
    // The link's audit endpoint pinned the withdrawn patch — L1 fails.
    let (_, out, _) = t.run(&["links", "list", "--json"]);
    let items = T::json(&out)["data"]["items"].as_array().cloned().unwrap();
    assert!(
        items.iter().any(|l| l["status"] == "withdrawn"),
        "link to a reset audit commit must be withdrawn: {items:?}"
    );
}

#[test]
fn reset_to_a_conclusion_commit_restores_that_conclusion() {
    let t = T::new();
    let (_ra, aid) = setup(&t);
    // init(pending) → fail → pass. Reset target = the fail commit: lands
    // ON fail, withdraws the pass patch. Effective conclusion must read
    // `fail`, not the Reset marker's absent conclusion.
    t.run(&["audit", "fail", &aid, "--text", "bad"]);
    let (_, out, _) = t.run(&["audit", "list", "--json"]);
    let fail_tip = T::json(&out)["data"]["items"][0]["tip"]
        .as_str()
        .unwrap()
        .to_string();
    t.run(&["audit", "pass", &aid, "--text", "fine"]);
    let (_, out, _) = t.run(&["audit", "show", &aid, "--json"]);
    assert_eq!(T::json(&out)["data"]["conclusion"], "pass");
    let (c, out, err) = t.run(&["commit", "reset", "a.md", "--reset-target", &fail_tip]);
    assert_eq!(c, 0, "{out} {err}");
    let (_, out, _) = t.run(&["audit", "show", &aid, "--json"]);
    assert_eq!(
        T::json(&out)["data"]["conclusion"],
        "fail",
        "reset landing on the fail commit must restore conclusion=fail"
    );
}

#[test]
fn unclean_to_fix_backlink_closure() {
    let t = T::new();
    let (ra, aid) = setup(&t);
    // A reason citing a nonexistent audit fails loud — the backtrace ref
    // must resolve, not be stored opaquely.
    let (c, _out, _err) = t.run(&[
        "commit",
        "unclean",
        "a.md",
        "--reason",
        "audit:deadbeef00deadbeef00 found a stale edge",
    ]);
    assert_ne!(c, 0, "unknown audit ref must be rejected");
    // audit finds a problem → unclean indexes it.
    let (c, out, err) = t.run(&[
        "commit",
        "unclean",
        "a.md",
        "--reason",
        &format!("audit:{aid} found a stale edge"),
    ]);
    assert_eq!(c, 0, "{out} {err}");
    // verify reports the dirty obligation.
    let (_, out, _) = t.run(&["verify", "a.md", "--json"]);
    let d = &T::json(&out)["data"];
    let dirty_blob = serde_json::to_string(d).unwrap();
    assert!(
        dirty_blob.contains("dirty"),
        "verify must report dirty: {d}"
    );
    // fix commit on the range + link back to the audit.
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "4", "--reason", "fix",
    ]);
    let (c, out, err) = t.run(&[
        "commit",
        "link",
        "a.md",
        "--source",
        &ra,
        "--target",
        &format!("audit:{aid}"),
        "--reason",
        "fix closes audit",
    ]);
    assert_eq!(c, 0, "{out} {err}");
    // audit show surfaces the back-link as link_ok.
    let (_, out, _) = t.run(&["audit", "show", &aid, "--json"]);
    let refs = T::json(&out)["data"]["references"]
        .as_array()
        .cloned()
        .unwrap();
    assert!(
        refs.iter().any(|r| r["status"] == "link_ok"),
        "back-link must show as link_ok: {refs:?}"
    );
}

#[test]
fn audit_list_time_filters_hit_and_miss() {
    let t = T::new();
    let (ra, aid) = setup(&t);
    // A link so the colored region covers a real endpoint version.
    t.write("b.md", "bbbb\n");
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "b.md", "--range", "0", "4"]);
    let rb = t.ranges_of("b.md").remove(0);
    let (c, out, err) = t.run(&[
        "commit", "link", "a.md", "--source", &ra, "--target", &rb, "--reason", "edge",
    ]);
    assert_eq!(c, 0, "{out} {err}");
    let _ = aid;
    // --start far in the past hits; --start in the far future misses.
    let (_, out, _) = t.run(&["audit", "list", "--start", "2000-01-01T00:00:00Z", "--json"]);
    assert_eq!(T::json(&out)["data"]["total"], 1, "past start must hit");
    let (_, out, _) = t.run(&["audit", "list", "--start", "2999-01-01T00:00:00Z", "--json"]);
    assert_eq!(T::json(&out)["data"]["total"], 0, "future start must miss");
    let (_, out, _) = t.run(&["audit", "list", "--end", "2999-01-01T00:00:00Z", "--json"]);
    assert_eq!(T::json(&out)["data"]["total"], 1, "future end must hit");
    // touched window far in the past → no endpoint commit falls inside.
    let (_, out, _) = t.run(&[
        "audit",
        "list",
        "--touched-start",
        "2000-01-01T00:00:00Z",
        "--touched-end",
        "2000-12-31T23:59:59Z",
        "--json",
    ]);
    assert_eq!(
        T::json(&out)["data"]["total"],
        0,
        "stale touched window must miss"
    );
    // touched window spanning now → endpoint versions qualify.
    let (_, out, _) = t.run(&[
        "audit",
        "list",
        "--touched-start",
        "2020-01-01T00:00:00Z",
        "--touched-end",
        "2999-12-31T23:59:59Z",
        "--json",
    ]);
    assert_eq!(
        T::json(&out)["data"]["total"],
        1,
        "wide touched window must hit"
    );
}
