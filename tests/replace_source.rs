//! Group 10.2/10.3: source replace — identical full content, shared-version
//! impact, binding revisions separate from original inputs.

mod common;
use std::path::Path;
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
            "omd-rp-{}-{}",
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
        let f = self.0.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, c).unwrap();
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
        let s = self.state();
        s.lines()
            .find(|l| l.contains(&format!("\"{node}\"")))
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

fn register_project(t: &T, alias: &str, project_root: &Path, tracked: &str) {
    let meta = project_root.join(".omd");
    let run = |args: &[&str]| {
        Command::new(omd())
            .arg("--json")
            .arg("--root")
            .arg(project_root)
            .arg("--meta")
            .arg(&meta)
            .args(args)
            .current_dir(project_root)
            .env("HOME", t.0.join("home"))
            .env("OMD_CONFIG_PATH", t.0.join("config"))
            .env("OMD_CACHE_PATH", t.0.join("cache"))
            .output()
            .unwrap()
    };
    let initialized = run(&["init", tracked]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stdout)
    );
    let observed = run(&["verify"]);
    let value: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    let expected = value["data"]["expected"].to_string();
    let registered = run(&[
        "--expected",
        &expected,
        "project",
        "register",
        alias,
        project_root.to_str().unwrap(),
        meta.to_str().unwrap(),
    ]);
    assert!(
        registered.status.success(),
        "{}",
        String::from_utf8_lossy(&registered.stdout)
    );
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn git_bound_source(
    with_equal_independent_version: bool,
) -> (T, std::path::PathBuf, String, String) {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    if with_equal_independent_version {
        t.write("b.md", "same");
        t.run(&["init", "b.md"]);
    }
    let commit = t.tip("file:a.md");
    let commit_record: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(t.0.join(format!(".omd/commits/{commit}.toml"))).unwrap(),
    )
    .unwrap();
    let version: omd::records::version::SourceVersion = toml::from_str(
        &std::fs::read_to_string(
            t.0.join(format!(".omd/versions/{}.toml", commit_record.content_ref)),
        )
        .unwrap(),
    )
    .unwrap();

    let repo = t.0.join("history");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("old.md"), "same").unwrap();
    git(&repo, &["add", "old.md"]);
    git(
        &repo,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "old"],
    );
    let old = git(&repo, &["rev-parse", "HEAD"]);
    register_project(&t, "history", &repo, "old.md");
    let (code, out, err) = t.run(&[
        "replace",
        &commit,
        "--source-type",
        "git",
        "--source-project",
        "history",
        "--git-commit",
        &old,
        "--git-path",
        "old.md",
    ]);
    assert_eq!(code, 0, "{out} {err}");
    (t, repo, old, version.sha256)
}

#[test]
fn replace_with_identical_content_succeeds() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let cid = t.tip("file:a.md");
    t.write("b.md", "same");
    let (c, out, _) = t.run(&[
        "replace",
        &cid,
        "--source-type",
        "file",
        "--source-path",
        &t.0.join("b.md").to_string_lossy(),
    ]);
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
    let (c, out, _) = t.run(&[
        "replace",
        &cid,
        "--source-type",
        "file",
        "--source-path",
        &t.0.join("b.md").to_string_lossy(),
    ]);
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
    t.run(&[
        "replace",
        &cid,
        "--source-type",
        "file",
        "--source-path",
        &t.0.join("b.md").to_string_lossy(),
    ]);
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
    t.run(&[
        "replace",
        &cid,
        "--source-type",
        "file",
        "--source-path",
        &t.0.join("b.md").to_string_lossy(),
    ]);
    // Same tip id, commit record still readable with original inputs.
    assert_eq!(t.tip("file:a.md"), cid, "commit id must be preserved");
    assert!(t.0.join(format!(".omd/commits/{cid}.toml")).exists());
}

#[test]
fn structural_commit_replace_rejects_without_guessing_basis() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let (rename, out, err) = t.run(&["rename", "a.md", "b.md"]);
    assert_eq!(rename, 0, "{out} {err}");
    std::fs::rename(t.0.join("a.md"), t.0.join("b.md")).unwrap();
    let marker = t.tip("file:b.md");
    t.write("candidate.md", "same");
    let before = t.state();
    let (code, out, err) = t.run(&[
        "replace",
        &marker,
        "--source-type",
        "file",
        "--source-path",
        "candidate.md",
    ]);
    assert_eq!(code, 2, "{out} {err}");
    assert_eq!(t.state(), before);
}

#[test]
fn shared_version_rebinds_together_other_version_untouched() {
    // Two retained records reference V; another object references equal-hash
    // W. Replacing V reports both V records and leaves W untouched.
    let t = T::new();
    t.write("a.md", "shared-content");
    t.write("b.md", "shared-content");
    t.run(&["init", "a.md"]);
    let init_a = t.tip("file:a.md");
    let (range_code, range_out, range_err) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "6", "--reason", "shared V",
    ]);
    assert_eq!(range_code, 0, "{range_out} {range_err}");
    let range_commit =
        serde_json::from_str::<serde_json::Value>(&range_out).unwrap()["data"]["commit"]
            .as_str()
            .unwrap()
            .to_string();
    t.run(&["init", "b.md"]);
    let tip_b = t.tip("file:b.md");
    t.write("new.md", "shared-content");
    let (c, out, _) = t.run(&[
        "replace",
        &init_a,
        "--source-type",
        "file",
        "--source-path",
        &t.0.join("new.md").to_string_lossy(),
    ]);
    assert_eq!(c, 0, "{out}");
    let response: serde_json::Value = serde_json::from_str(&out).unwrap();
    let affected = response["data"]["affected"].as_array().unwrap();
    assert!(affected.iter().any(|id| id == &init_a));
    assert!(affected.iter().any(|id| id == &range_commit));
    assert!(!affected.iter().any(|id| id == &tip_b));
    assert_ne!(tip_b, init_a);
    assert!(t.state().contains(&tip_b));
}

#[test]
fn file_alias_replace_keeps_current_observation_on_original_file() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let (range_code, range_out, range_err) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "4", "--reason", "range",
    ]);
    assert_eq!(range_code, 0, "{range_out} {range_err}");
    let commit = serde_json::from_str::<serde_json::Value>(&range_out).unwrap()["data"]["commit"]
        .as_str()
        .unwrap()
        .to_string();
    let other = t.0.join("other-project");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("recovery.md"), "same").unwrap();
    register_project(&t, "other", &other, "recovery.md");
    let (code, out, err) = t.run(&[
        "replace",
        &commit,
        "--source-type",
        "file",
        "--source-project",
        "other",
        "--source-path",
        "recovery.md",
    ]);
    assert_eq!(code, 0, "{out} {err}");
    std::fs::write(other.join("recovery.md"), "DIFF").unwrap();
    let (clean, clean_out, clean_err) = t.run(&["verify", "a.md"]);
    assert_eq!(
        clean, 0,
        "recovery file is not current observation: {clean_out} {clean_err}"
    );
    t.write("a.md", "DIFF");
    let (dirty, dirty_out, dirty_err) = t.run(&["verify", "a.md"]);
    assert_eq!(
        dirty, 1,
        "current file still controls observation: {dirty_out} {dirty_err}"
    );
}

#[test]
fn command_replace_executes_once_and_never_becomes_current_observation() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let commit = t.tip("file:a.md");
    let script = t.0.join("recovery.sh");
    let counter = t.0.join("count");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nn=0\n[ ! -f '{}' ] || n=$(cat '{}')\necho $((n+1)) > '{}'\nprintf same\n",
            counter.display(),
            counter.display(),
            counter.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (code, out, err) = t.run(&[
        "replace",
        &commit,
        "--source-type",
        "command",
        "--executable",
        script.to_str().unwrap(),
        "--args-json",
        "[]",
    ]);
    assert_eq!(code, 0, "{out} {err}");
    assert_eq!(std::fs::read_to_string(&counter).unwrap().trim(), "1");
    let (verify, verify_out, verify_err) = t.run(&["verify", "a.md"]);
    assert_eq!(verify, 0, "{verify_out} {verify_err}");
    assert_eq!(std::fs::read_to_string(&counter).unwrap().trim(), "1");
}

#[test]
fn gc_releases_content_only_with_exact_git_recovery() {
    let (t, _, _, hash) = git_bound_source(false);
    let (code, out, err) = t.run(&["gc", "--content"]);
    assert_eq!(code, 0, "{out} {err}");
    assert!(!t.0.join(format!(".omd/content/{hash}")).exists());
    assert!(out.contains(&hash));
    let (verify, verify_out, verify_err) = t.run(&["verify", "a.md"]);
    assert_eq!(
        verify, 0,
        "exact Git binding recovers released content: {verify_out} {verify_err}"
    );
}

#[test]
fn gc_keeps_equal_hash_independent_version_without_git_recovery() {
    let (t, _, _, hash) = git_bound_source(true);
    let (code, out, err) = t.run(&["gc", "--content"]);
    assert_eq!(code, 0, "{out} {err}");
    assert!(t.0.join(format!(".omd/content/{hash}")).exists());
}

#[test]
fn gc_keeps_content_when_exact_git_object_is_unavailable() {
    let (t, repo, commit, hash) = git_bound_source(false);
    let object = repo
        .join(".git/objects")
        .join(&commit[..2])
        .join(&commit[2..]);
    std::fs::remove_file(object).unwrap();
    let (code, out, err) = t.run(&["gc", "--content"]);
    assert_eq!(code, 0, "{out} {err}");
    assert!(t.0.join(format!(".omd/content/{hash}")).exists());
}

#[test]
fn corrupt_binding_rejects_gc_without_releasing_content() {
    let (t, _, commit, hash) = git_bound_source(false);
    let binding = std::fs::read_dir(t.0.join(".omd/bindings"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = std::fs::read_to_string(&binding).unwrap();
    std::fs::write(&binding, text.replace(&commit, "HEAD")).unwrap();
    let before = t.state();
    let (code, _, _) = t.run(&["gc", "--content"]);
    assert_ne!(code, 0);
    assert_eq!(t.state(), before);
    assert!(t.0.join(format!(".omd/content/{hash}")).exists());
}

#[test]
fn exact_git_alias_replace_ignores_head_and_current_file_stays_authoritative() {
    let t = T::new();
    t.write("a.md", "same");
    t.run(&["init", "a.md"]);
    let (range_code, range_out, range_err) = t.run(&[
        "commit", "commit", "a.md", "--range", "0", "4", "--reason", "range",
    ]);
    assert_eq!(range_code, 0, "{range_out} {range_err}");
    let commit = serde_json::from_str::<serde_json::Value>(&range_out).unwrap()["data"]["commit"]
        .as_str()
        .unwrap()
        .to_string();

    let repo = t.0.join("history");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("old.md"), "same").unwrap();
    git(&repo, &["add", "old.md"]);
    git(
        &repo,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "old"],
    );
    let old = git(&repo, &["rev-parse", "HEAD"]);
    register_project(&t, "history", &repo, "old.md");

    let (code, out, err) = t.run(&[
        "replace",
        &commit,
        "--source-type",
        "git",
        "--source-project",
        "history",
        "--git-commit",
        &old,
        "--git-path",
        "old.md",
    ]);
    assert_eq!(code, 0, "{out} {err}");

    std::fs::write(repo.join("old.md"), "DIFF").unwrap();
    git(&repo, &["add", "old.md"]);
    git(
        &repo,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "head moved"],
    );
    let (clean, clean_out, clean_err) = t.run(&["verify", "a.md"]);
    assert_eq!(
        clean, 0,
        "HEAD never substitutes for current file: {clean_out} {clean_err}"
    );

    t.write("a.md", "DIFF");
    let (dirty, dirty_out, dirty_err) = t.run(&["verify", "a.md"]);
    assert_eq!(
        dirty, 1,
        "current uncommitted bytes stay authoritative: {dirty_out} {dirty_err}"
    );

    let before = t.state();
    let missing = "0".repeat(40);
    let (failed, _, _) = t.run(&[
        "replace",
        &commit,
        "--source-type",
        "git",
        "--source-project",
        "history",
        "--git-commit",
        &missing,
        "--git-path",
        "old.md",
    ]);
    assert_ne!(failed, 0);
    assert_eq!(
        t.state(),
        before,
        "missing historical object publishes nothing"
    );

    std::fs::remove_file(t.0.join("a.md")).unwrap();
    let (missing_current, missing_out, missing_err) = t.run(&["verify", "a.md"]);
    assert_eq!(
        missing_current, 1,
        "readable Git history never replaces missing current file: {missing_out} {missing_err}"
    );
    assert!(missing_out.contains("missing") || missing_out.contains("unverified"));
}
