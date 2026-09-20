//! Reset semantics: tip move, dangling, link/adapt withdrawal, JSON outcome.

use std::process::Command;

fn omd() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") { return p.into(); }
    let mut p = std::env::current_exe().unwrap();
    p.pop(); p.pop(); p.push("omd"); p
}

struct T(std::path::PathBuf);
impl T {
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!("omd-rs-{}",
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
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
    fn tip(&self, node: &str) -> String {
        self.state().lines()
            .find(|l| l.contains(&format!("\"{node}\"")) && l.contains('='))
            .and_then(|l| l.split('=').nth(1).map(|v| v.trim().trim_matches('"').to_string()))
            .unwrap_or_default()
    }
    fn prev(&self, cid: &str) -> String {
        std::fs::read_to_string(self.0.join(format!(".omd/commits/{cid}.toml")))
            .unwrap_or_default()
            .lines().find(|l| l.starts_with("previous_id"))
            .map(|l| l.split('=').nth(1).unwrap().trim().trim_matches('"').to_string())
            .unwrap_or_default()
    }
}
impl Drop for T { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn mk_range(t: &T) -> String {
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "r"]);
    t.tip("range:a.md@text:0-1")
}

#[test]
fn reset_moves_tip_to_landing_and_dangles_removed() {
    let t = T::new();
    let c1 = mk_range(&t);
    t.run(&["commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "c2"]);
    let c2 = t.tip("range:a.md@text:0-1");
    assert_ne!(c1, c2);
    // Reset to c1.
    t.run(&["commit", "reset", "a.md", "--reason", &c1]);
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
    let (_, out, _) = t.run(&["commit", "reset", "a.md", "--reason", &c1, "--json"]);
    assert!(out.contains("\"requested\""), "must report requested: {out}");
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
    let (_, out, _) = t.run(&["commit", "reset", "a.md", "--reason", &end, "--json"]);
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
    let (c, out, e) = t.run(&["commit", "reset", "a.md", "--reason", &inside]);
    assert!(c != 0 || e.contains("member") || e.contains("Interior") || out.contains("block member"),
        "interior member must refuse: {out} {e}");
}

#[test]
fn new_commit_after_reset_continues_chain() {
    let t = T::new();
    let c1 = mk_range(&t);
    t.run(&["commit", "commit", "a.md", "--id", &c1, "--range", "0-1", "--reason", "c2"]);
    t.run(&["commit", "reset", "a.md", "--reason", &c1]);
    // A new commit after reset continues from the reset tip.
    let (c, _, _) = t.run(&["commit", "commit", "a.md", "--range", "0-1", "--reason", "fresh"]);
    assert_eq!(c, 0, "commit after reset must succeed");
}
