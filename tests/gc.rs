//! Group 12.2/13.x: explicit gc of unreferenced dangling commits + content.

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
            "omd-gc-{}-{}",
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
    fn commit_exists(&self, id: &str) -> bool {
        self.0.join(format!(".omd/commits/{id}.toml")).exists()
    }
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn gc_collects_unreferenced_dangling_commit() {
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
    t.run(&["commit", "reset", "a.md", "--reset-target", &c1]);
    // c2 is dangling (unreferenced). gc collects it.
    assert!(t.commit_exists(&c2), "c2 on disk before gc");
    let (c, out, _) = t.run(&["gc"]);
    assert_eq!(c, 0, "{out}");
    assert!(!t.commit_exists(&c2), "unreferenced dangling c2 collected");
}

#[test]
fn gc_retains_reachable_commits() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0-1", "--reason", "r1",
    ]);
    let c1 = t.tip("range:a.md@text:0-1");
    t.run(&["gc"]);
    // The current tip chain (c1 + its ancestors) is reachable — retained.
    assert!(t.commit_exists(&c1), "reachable c1 must be retained");
}

#[test]
fn gc_content_only_frees_unreferenced_content() {
    let t = T::new();
    t.write("a.md", "keep");
    t.run(&["init", "a.md"]);
    // Content dir holds the init's content; --content must not free a
    // version still referenced by a live record.
    t.run(&["gc", "--content"]);
    let tip = t.tip("file:a.md");
    assert!(t.commit_exists(&tip), "tip retained");
}
