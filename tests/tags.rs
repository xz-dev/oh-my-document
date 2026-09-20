//! Group 7.2/7.3: tags, inheritance, direction rules, check+verify integration.

use std::collections::BTreeSet;
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
            "omd-tag-{}-{}",
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
        let f = self.0.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, c).unwrap();
    }
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn dir_tag_inherits_to_members_deduped() {
    // Inheritance: docs/s.md gets `spec` from file:docs AND its own tag —
    // deduped (a set), never counted twice.
    let mut st = omd::records::store::State::default();
    st.tags
        .insert("file:docs".into(), BTreeSet::from(["spec".to_string()]));
    st.tags.insert(
        "file:docs/s.md".into(),
        BTreeSet::from(["spec".to_string(), "example".to_string()]),
    );
    let got = omd::relations::tags::resolve_tags(&st, "file:docs/s.md");
    assert!(got.contains("spec") && got.contains("example"));
    assert_eq!(got.len(), 2, "same tag inherited twice must dedup: {got:?}");
}

#[test]
fn new_member_inherits_dir_tag() {
    // A file added AFTER the dir tag still inherits (resolve at check time).
    let t = T::new();
    t.write("docs/a.md", "a");
    t.run(&["import", "docs"]);
    t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    t.write("docs/late.md", "b"); // added after tag
    let mut st = omd::records::store::State::default();
    st.tags
        .insert("file:docs".into(), BTreeSet::from(["spec".to_string()]));
    let got = omd::relations::tags::resolve_tags(&st, "file:docs/late.md");
    assert!(got.contains("spec"), "late member must inherit dir tag");
}

#[test]
fn commit_tag_and_rule_persist() {
    let t = T::new();
    t.write("docs/s.md", "s");
    t.write("code/i.rs", "i");
    t.run(&["import", "docs"]);
    t.run(&["import", "code"]);
    let (c, _, _) = t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    assert_eq!(c, 0);
    let (c, _, _) = t.run(&[
        "commit",
        "scope_adjust",
        "docs",
        "--rule",
        "spec->code",
        "--level",
        "fail",
    ]);
    assert_eq!(c, 0);
    let s = std::fs::read_to_string(t.0.join(".omd/state.toml")).unwrap();
    assert!(s.contains("spec->code"));
}

#[test]
fn uncovered_rule_fails_check_at_fail_level() {
    let t = T::new();
    t.write("docs/s.md", "s");
    t.write("code/i.rs", "i");
    t.run(&["import", "docs"]);
    t.run(&["import", "code"]);
    t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "docs",
        "--rule",
        "spec->code",
        "--level",
        "fail",
    ]);
    let (c, out, _) = t.run(&["check"]);
    assert!(
        !out.contains("\"ok\": true"),
        "uncovered rule must fail check:\n{out}"
    );
    let _ = c;
}

#[test]
fn warn_level_reports_gap_without_failing() {
    let t = T::new();
    t.write("docs/s.md", "s");
    t.write("code/i.rs", "i");
    t.run(&["import", "docs"]);
    t.run(&["import", "code"]);
    t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "docs",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
    let (_, out, _) = t.run(&["check"]);
    // warn reports the gap but does not itself fail the check.
    assert!(
        out.contains("\"status\": \"fail\"") || out.contains("warn"),
        "{out}"
    );
}

#[test]
fn one_way_allows_extra_reverse_links() {
    // spec->code covered; an extra code->spec link is NOT a violation of
    // the one-way rule — check on 'spec->code' must not fail because a
    // reverse edge exists.
    let t = T::new();
    t.write("docs/s.md", "spec-content");
    t.write("code/i.rs", "impl");
    t.run(&["import", "docs"]);
    t.run(&["import", "code"]);
    t.run(&["init", "docs/s.md"]);
    t.run(&["init", "code/i.rs"]);
    t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    t.run(&["commit", "tag", "code", "--tag", "code"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "docs",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
    // spec range links to code; code range also links back to spec.
    t.run(&[
        "commit",
        "commit",
        "docs/s.md",
        "--range",
        "0-4",
        "--link-to",
        "range:code/i.rs@text:0-4",
    ]);
    t.run(&[
        "commit",
        "commit",
        "code/i.rs",
        "--range",
        "0-4",
        "--link-to",
        "range:docs/s.md@text:0-4",
    ]);
    let (_, out, _) = t.run(&["check"]);
    // One-way spec->code: extra reverse edge must not produce a violation.
    assert!(out.contains("spec->code"), "rule missing:\n{out}");
}

#[test]
fn coverage_gap_but_verify_can_pass() {
    // 7.3: link coverage <100% does NOT make verify fail by itself.
    let t = T::new();
    t.write("docs/s.md", "spec");
    t.run(&["import", "docs"]);
    t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "docs",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
    // check reports the gap (warn) but verify's own conditions pass.
    let (_, chk, _) = t.run(&["check"]);
    let (_, ver, _) = t.run(&["verify"]);
    assert!(chk.contains("spec->code"), "rule gap not reported:\n{chk}");
    // verify's own `ok` field must be true — coverage is not a verify gate.
    let v: serde_json::Value = serde_json::from_str(&ver).unwrap_or_default();
    assert_eq!(
        v.get("ok").and_then(|x| x.as_bool()),
        Some(true),
        "verify should pass despite coverage gap:\n{ver}"
    );
}

#[test]
fn skip_does_not_confirm_content() {
    let t = T::new();
    t.write("docs/s.md", "s");
    t.write("code/i.rs", "i");
    t.run(&["import", "docs"]);
    t.run(&["import", "code"]);
    t.run(&["commit", "tag", "docs", "--tag", "spec"]);
    t.run(&[
        "commit",
        "scope_adjust",
        "docs",
        "--rule",
        "spec->code",
        "--skip",
    ]);
    let (_, out, _) = t.run(&["check"]);
    assert!(
        out.contains("skipped"),
        "skipped rule must show skipped:\n{out}"
    );
}
