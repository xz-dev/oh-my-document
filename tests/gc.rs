//! Group 12.2/13.x: explicit gc of unreferenced dangling commits + content.

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
    fn tip(&self, node: &str) -> String {
        let node = node
            .strip_prefix("file:")
            .map(|path| self.file_node(path))
            .filter(|node| !node.is_empty())
            .unwrap_or_else(|| node.to_string());
        self.state()
            .lines()
            .find(|l| l.contains(&format!("\"{node}\"")) && l.contains('='))
            .and_then(|l| {
                l.split('=')
                    .nth(1)
                    .map(|v| v.trim().trim_matches('"').to_string())
            })
            .unwrap_or_default()
    }
    fn range_node(&self, file: &str) -> String {
        let state: omd::records::store::State = toml::from_str(&self.state()).unwrap();
        let file_node = self.file_node(file);
        state
            .mounts
            .get(&file_node)
            .and_then(|children| children.first())
            .cloned()
            .unwrap_or_default()
    }
    /// The first range node mounted under `file:<path>` — the key is the
    /// chain-root commit id, not a coordinate string.
    fn range_tip(&self, file: &str) -> String {
        let s = self.state();
        let file_node = self.file_node(file);
        let mut range_key = String::new();
        for l in s.lines() {
            if l.contains(&format!("\"{file_node}\"")) && l.contains('[') {
                if let Some(pos) = l.find("range:") {
                    let rest = &l[pos..];
                    if let Some(end) = rest.find('"') {
                        range_key = rest[..end].to_string();
                    }
                }
            }
        }
        if range_key.is_empty() {
            return String::new();
        }
        s.lines()
            .find(|l| l.starts_with(&format!("\"{range_key}\"")) && l.contains('='))
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
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r1",
    ]);
    let c1 = t.range_tip("a.md");
    t.run(&[
        "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "r2",
    ]);
    let c2 = t.range_tip("a.md");
    t.run(&["commit", "reset", "a.md", "--reset-target", &c1]);
    // c2 is dangling (unreferenced). gc collects it.
    assert!(t.commit_exists(&c2), "c2 on disk before gc");
    let (c, out, _) = t.run(&["gc"]);
    assert_eq!(c, 0, "{out}");
    assert!(!t.commit_exists(&c2), "unreferenced dangling c2 collected");
}

#[test]
fn gc_removes_collected_commit_from_dangling_and_retained_indexes() {
    let t = T::new();
    t.write("a.md", "a");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r1",
        ])
        .0,
        0
    );
    let c1 = t.range_tip("a.md");
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--id", &c1, "--range", "0", "1", "--reason", "r2",
        ])
        .0,
        0
    );
    let c2 = t.range_tip("a.md");
    assert_eq!(
        t.run(&["commit", "reset", "a.md", "--reset-target", &c1]).0,
        0
    );
    let (gc_code, gc_out, gc_err) = t.run(&["gc"]);
    assert_eq!(gc_code, 0, "gc failed: {gc_out} {gc_err}");
    assert!(
        gc_out.contains(&c2),
        "gc must report collected commit: {gc_out}"
    );
    let (list_code, dangling, list_err) = t.run(&["list", "--dangling"]);
    assert_eq!(list_code, 0, "list failed after gc: {list_err}");
    assert!(
        !dangling.contains(&c2),
        "collected commit still listed: {dangling}"
    );
    assert!(
        !t.state().contains(&c2),
        "collected commit remains in state indexes"
    );
    let (log_code, log_out, log_err) = t.run(&["log", &c1]);
    assert_eq!(
        log_code, 0,
        "retained commit not loggable: {log_out} {log_err}"
    );
}

#[test]
fn gc_retains_reachable_commits() {
    let t = T::new();
    t.write("a.md", "a");
    t.run(&["init", "a.md"]);
    t.run(&[
        "commit", "commit", "a.md", "--range", "0", "1", "--reason", "r1",
    ]);
    let c1 = t.range_tip("a.md");
    t.run(&["gc"]);
    // The current tip chain (c1 + its ancestors) is reachable — retained.
    assert!(t.commit_exists(&c1), "reachable c1 must be retained");
}

#[test]
fn gc_protects_child_tip_saved_by_live_file_snapshot() {
    let t = T::new();
    t.write("a.md", "a");
    t.write("b.md", "b");
    assert_eq!(t.run(&["init", "a.md"]).0, 0);
    assert_eq!(t.run(&["init", "b.md"]).0, 0);
    assert_eq!(
        t.run(&[
            "commit", "commit", "a.md", "--range", "0", "1", "--reason", "source",
        ])
        .0,
        0
    );
    let source = t.range_node("a.md");
    assert_eq!(
        t.run(&[
            "commit",
            "commit",
            "b.md",
            "--range",
            "0",
            "1",
            "--link-from",
            &source,
            "--reason",
            "combo",
        ])
        .0,
        0
    );
    let child = t.range_node("b.md");
    let child_root = child.strip_prefix("range:").unwrap().to_string();
    let saved_tip = t.range_tip("b.md");
    assert_eq!(t.run(&["commit", "verify", "b.md"]).0, 0);
    assert_eq!(
        t.run(&["commit", "reset", "b.md", "--reset-target", &child_root,])
            .0,
        0
    );
    assert!(t.range_node("b.md").is_empty(), "child must be unmounted");
    assert_eq!(t.run(&["gc"]).0, 0);
    assert!(
        t.commit_exists(&saved_tip),
        "live file snapshot must protect saved child tip"
    );
    let (code, out, err) = t.run(&["log", &saved_tip]);
    assert_eq!(
        code, 0,
        "protected saved child must remain loggable: {out} {err}"
    );
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
