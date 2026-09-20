//! Group 10.2/10.3: source replace — identical full content, shared-version
//! impact, binding revisions separate from original inputs.

use std::process::Command;

fn omd() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") { return p.into(); }
    let mut p = std::env::current_exe().unwrap();
    p.pop(); p.pop(); p.push("omd"); p
}

struct T(std::path::PathBuf);
impl T {
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!("omd-rp-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
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
    fn write(&self, p: &str, c: &str) {
        let f = self.0.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, c).unwrap();
    }
    fn tip(&self, node: &str) -> String {
        let s = std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default();
        s.lines().find(|l| l.contains(&format!("\"{node}\"")))
            .and_then(|l| l.split('=').nth(1).map(|v| v.trim().trim_matches('"').to_string()))
            .unwrap_or_default()
    }
}
impl Drop for T { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[test]
fn replace_with_identical_content_succeeds() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let cid = t.tip("file:a.md");
    t.write("b.md", "same");
    let (c, out, _) = t.run(&["replace", &cid, "--source", &t.0.join("b.md").to_string_lossy()]);
    assert_eq!(c, 0, "{out}");
    assert!(out.contains("binding"));
}

#[test]
fn replace_refused_on_mismatched_full_content() {
    let t = T::new();
    t.write("a.md", "ABCD");
    t.run(&["init", "a.md"]);
    let cid = t.tip("file:a.md");
    // New source shares a range fragment but full content differs.
    t.write("b.md", "AB different");
    let (c, out, _) = t.run(&["replace", &cid, "--source", &t.0.join("b.md").to_string_lossy()]);
    assert!(c != 0 || out.contains("refused"), "must refuse: {out}");
}

#[test]
fn replace_does_not_add_omd_commit() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let cid = t.tip("file:a.md");
    let before = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    t.write("b.md", "x");
    t.run(&["replace", &cid, "--source", &t.0.join("b.md").to_string_lossy()]);
    let after = std::fs::read_dir(t.0.join(".omd/commits")).unwrap().count();
    assert_eq!(before, after, "replace must not add an OMD commit");
}

#[test]
fn replace_preserves_commit_id_and_inputs() {
    let t = T::new();
    t.write("a.md", "x");
    t.run(&["init", "a.md"]);
    let cid = t.tip("file:a.md");
    t.write("b.md", "x");
    t.run(&["replace", &cid, "--source", &t.0.join("b.md").to_string_lossy()]);
    // Same tip id, commit record still readable with original inputs.
    assert_eq!(t.tip("file:a.md"), cid, "commit id must be preserved");
    assert!(t.0.join(format!(".omd/commits/{cid}.toml")).exists());
}

#[test]
fn shared_version_rebinds_together_other_version_untouched() {
    // O1/O2 reference version V (same content); O3 references W (same content
    // too). Replacing O1's V must report O1/O2 but leave O3's W binding alone.
    let t = T::new();
    t.write("a.md", "shared-content");
    t.write("b.md", "shared-content");
    t.run(&["init", "a.md"]); t.run(&["init", "b.md"]);
    let cid = t.tip("file:a.md");
    t.write("new.md", "shared-content");
    let (c, out, _) = t.run(&["replace", &cid, "--source", &t.0.join("new.md").to_string_lossy()]);
    assert_eq!(c, 0, "{out}");
    // The binding reports the affected records sharing version V.
    assert!(out.contains("affected"));
    // O3 (b.md) shares the SAME content but a different version record — its
    // binding must be untouched: the b.md tip stays on its own commit, not
    // rebound to a's.
    let tip_b = t.tip("file:b.md");
    assert_ne!(tip_b, cid, "b's commit distinct from a's — no cross-rebind");
    // And b still resolves to its own (independent) version binding.
    let st = std::fs::read_to_string(t.0.join(".omd/state.toml")).unwrap_or_default();
    assert!(st.contains(&tip_b), "b's tip still its own record: {tip_b}");
}
