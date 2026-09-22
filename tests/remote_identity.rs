mod common;

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use omd::records::registration::RemoteIdentity;
use omd::records::store::{Expected, Store, StoreError};
use omd::sources::projects;

fn omd() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_omd") {
        return path.into();
    }
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("omd");
    path
}

struct Env {
    root: tempfile::TempDir,
    config: PathBuf,
    cache: PathBuf,
    home: PathBuf,
    git_global: PathBuf,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config");
        let cache = root.path().join("cache");
        let home = root.path().join("home");
        let git_global = root.path().join("gitconfig");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&git_global, "").unwrap();
        Self {
            root,
            config,
            cache,
            home,
            git_global,
        }
    }

    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(omd());
        command
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("OMD_CONFIG_PATH", &self.config)
            .env("OMD_CACHE_PATH", &self.cache)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.git_global);
        command
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.command(cwd)
            .args(common::with_expected(
                &omd(),
                cwd,
                args,
                None,
                Some(&self.home),
                Some(&self.config),
                Some(&self.cache),
            ))
            .output()
            .unwrap()
    }

    fn run_raw(&self, cwd: &Path, args: &[String]) -> Output {
        self.command(cwd).args(args).output().unwrap()
    }
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git(env: &Env, cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("HOME", &env.home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &env.git_global)
        .status()
        .unwrap();
    assert!(status.success());
}

fn fixture(env: &Env, remote: &str) -> (PathBuf, PathBuf) {
    let project = env.root.path().join("project");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("a.md"), "a\n").unwrap();
    git(env, &project, &["init", "-q"]);
    git(env, &project, &["config", "remote.upstream.url", remote]);
    assert_ok(&env.run(
        &project,
        &["--meta", meta.to_str().unwrap(), "init", "a.md"],
    ));
    (project, meta)
}

fn register(env: &Env, project: &Path, meta: &Path, url: Option<&str>) -> Output {
    let observed = env.run(
        project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--json",
            "verify",
        ],
    );
    let value: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    let evidence = value["data"]["expected"].to_string();
    let mut args = vec![
        "--expected",
        evidence.as_str(),
        "project",
        "register",
        "app",
        project.to_str().unwrap(),
        meta.to_str().unwrap(),
    ];
    if let Some(url) = url {
        args.extend(["--git-remote", "upstream", "--git-remote-url", url]);
    }
    env.run(project, &args)
}

fn expected(env: &Env, project: &Path) -> serde_json::Value {
    let output = env.run(project, &["--project", "app", "--json", "verify"]);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    value["data"]["expected"].clone()
}

fn recognize(env: &Env, project: &Path, url: &str) -> Output {
    let evidence = expected(env, project).to_string();
    env.run(
        project,
        &[
            "--expected",
            evidence.as_str(),
            "project",
            "recognize-remote",
            "app",
            "--git-remote-url",
            url,
        ],
    )
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn state_registration(meta: &Path) -> String {
    let value: toml::Value =
        toml::from_str(&std::fs::read_to_string(meta.join("state.toml")).unwrap()).unwrap();
    value["registrations"]["project:app"]
        .as_str()
        .unwrap()
        .to_string()
}

fn snapshot(paths: &[&Path]) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(base: &Path, path: &Path, rows: &mut Vec<(PathBuf, Vec<u8>)>) {
        if !path.exists() {
            return;
        }
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, rows);
            } else {
                rows.push((
                    path.strip_prefix(base).unwrap_or(&path).to_path_buf(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut rows = Vec::new();
    for path in paths {
        walk(path.parent().unwrap_or(path), path, &mut rows);
    }
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    rows
}

#[test]
fn unconstrained_non_git_project_never_invokes_git() {
    let env = Env::new();
    let project = env.root.path().join("plain");
    let meta = project.join(".omd");
    let bin = env.root.path().join("bin");
    let log = env.root.path().join("git-called");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(project.join("a.md"), "a\n").unwrap();
    let trap = bin.join("git");
    std::fs::write(
        &trap,
        format!("#!/bin/sh\necho called >> '{}'\nexit 99\n", log.display()),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&trap).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&trap, permissions).unwrap();

    let mut verify_output = None;
    for args in [
        vec!["--meta", meta.to_str().unwrap(), "init", "a.md"],
        vec!["--meta", meta.to_str().unwrap(), "verify"],
        vec!["--meta", meta.to_str().unwrap(), "check"],
    ] {
        let is_verify = args.last() == Some(&"verify");
        let output = env
            .command(&project)
            .env("PATH", &bin)
            .args(args)
            .output()
            .unwrap();
        assert_ok(&output);
        if is_verify {
            verify_output = Some(output);
        }
    }
    let verify: serde_json::Value = serde_json::from_slice(&verify_output.unwrap().stdout).unwrap();
    let expected_json = verify["data"]["expected"].to_string();
    let write = env
        .command(&project)
        .env("PATH", &bin)
        .args([
            "--meta",
            meta.to_str().unwrap(),
            "--expected",
            &expected_json,
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "plain update",
        ])
        .output()
        .unwrap();
    assert_ok(&write);
    assert!(!log.exists());
}

#[test]
fn registration_is_immutable_and_pair_is_required() {
    let env = Env::new();
    let (project, meta) = fixture(&env, "ssh://git@example.invalid:22/Org/Repo.git");
    let before = std::fs::read(meta.join("state.toml")).unwrap();
    let incomplete = env.run(
        &project,
        &[
            "project",
            "register",
            "app",
            project.to_str().unwrap(),
            meta.to_str().unwrap(),
            "--git-remote",
            "upstream",
        ],
    );
    assert_eq!(incomplete.status.code(), Some(2));
    assert_eq!(std::fs::read(meta.join("state.toml")).unwrap(), before);
    let empty = env.run(
        &project,
        &[
            "project",
            "register",
            "app",
            project.to_str().unwrap(),
            meta.to_str().unwrap(),
            "--git-remote",
            "",
            "--git-remote-url",
            "ssh://git@example.invalid:22/Org/Repo.git",
        ],
    );
    assert_eq!(empty.status.code(), Some(2));
    assert_eq!(std::fs::read(meta.join("state.toml")).unwrap(), before);

    assert_ok(&register(
        &env,
        &project,
        &meta,
        Some("ssh://git@example.invalid:22/Org/Repo.git"),
    ));
    let tips_before: toml::Value =
        toml::from_str(&std::fs::read_to_string(meta.join("state.toml")).unwrap()).unwrap();
    let old_id = state_registration(&meta);
    let old_state = std::fs::read(meta.join("state.toml")).unwrap();
    let old_bytes = std::fs::read(meta.join(format!("registrations/{old_id}.toml"))).unwrap();
    assert_ok(&register(
        &env,
        &project,
        &meta,
        Some("https://example.invalid/Org/Repo.git"),
    ));
    let new_id = state_registration(&meta);
    assert_ne!(old_id, new_id);
    assert_eq!(
        std::fs::read(meta.join(format!("registrations/{old_id}.toml"))).unwrap(),
        old_bytes
    );
    let selected: toml::Value =
        toml::from_str(&std::fs::read_to_string(meta.join("state.toml")).unwrap()).unwrap();
    assert_eq!(
        selected["registrations"]["project:app"].as_str(),
        Some(new_id.as_str())
    );
    assert_eq!(selected["tips"], tips_before["tips"]);
    let pinned = env.root.path().join("pinned-old-authority");
    copy_tree(&meta, &pinned);
    std::fs::write(pinned.join("state.toml"), old_state).unwrap();
    let pinned_registration = omd::records::store::Store::open_existing(&pinned)
        .unwrap()
        .project_registration("app")
        .unwrap();
    assert_eq!(
        pinned_registration.remote.unwrap().url,
        "ssh://git@example.invalid:22/Org/Repo.git"
    );
    let record =
        std::fs::read_to_string(meta.join(format!("registrations/{new_id}.toml"))).unwrap();
    assert!(record.contains("https://example.invalid/Org/Repo.git"));
    assert!(!record.contains(project.to_str().unwrap()));
}

#[test]
fn raw_remote_match_is_exact_and_local_recognition_is_instance_scoped() {
    let env = Env::new();
    let raw = "git@example.invalid:Org/Repo.git";
    let canonical = "https://example.invalid/Org/Repo.git";
    let (project, meta) = fixture(&env, raw);
    git(
        &env,
        &project,
        &[
            "config",
            "url.https://example.invalid/.insteadOf",
            "git@example.invalid:",
        ],
    );
    assert_ok(&register(&env, &project, &meta, Some(canonical)));

    let mismatch = env.run(&project, &["--project", "app", "--json", "verify"]);
    assert_eq!(mismatch.status.code(), Some(1));
    let text = String::from_utf8_lossy(&mismatch.stdout);
    assert!(text.contains("remote upstream URL mismatch"), "{text}");
    let mismatch_value: serde_json::Value = serde_json::from_slice(&mismatch.stdout).unwrap();
    assert!(
        mismatch_value["data"]["expected"]["acquisition_versions"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let check = env.run(&project, &["--project", "app", "--json", "check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("remote upstream URL mismatch"));

    let listed = env.run(&project, &["project", "list"]);
    assert_ok(&listed);
    assert!(String::from_utf8_lossy(&listed.stdout).contains(canonical));

    assert_ok(&recognize(&env, &project, raw));
    assert_ok(&env.run(&project, &["--project", "app", "verify"]));
    git(
        &env,
        &project,
        &[
            "config",
            "remote.upstream.url",
            "git@example.invalid:Other/Repo.git",
        ],
    );
    assert_eq!(
        env.run(&project, &["--project", "app", "verify"])
            .status
            .code(),
        Some(1)
    );
    git(&env, &project, &["config", "remote.upstream.url", raw]);

    assert_ok(&register(
        &env,
        &project,
        &meta,
        Some("https://example.invalid/New/Repo.git"),
    ));
    assert_eq!(
        env.run(&project, &["--project", "app", "verify"])
            .status
            .code(),
        Some(1)
    );
    assert_ok(&register(&env, &project, &meta, Some(canonical)));

    let other = Env::new();
    assert_ok(&register(&other, &project, &meta, Some(canonical)));
    let not_shared = other.run(&project, &["--project", "app", "verify"]);
    assert_eq!(not_shared.status.code(), Some(1));
}

#[test]
fn missing_username_case_port_and_transport_changes_fail_closed() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    assert_ok(&env.run(&project, &["--project", "app", "verify"]));

    git(
        &env,
        &project,
        &[
            "config",
            "--add",
            "remote.upstream.url",
            "ssh://git@example.invalid:22/Org/Other.git",
        ],
    );
    let ambiguous = env.run(&project, &["--project", "app", "verify"]);
    assert_eq!(ambiguous.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&ambiguous.stdout).contains("ambiguous configured URLs"));

    git(
        &env,
        &project,
        &["config", "--unset-all", "remote.upstream.url"],
    );
    let missing = env.run(&project, &["--project", "app", "verify"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stdout).contains("absent or unreadable"));

    for wrong in [
        "ssh://other@example.invalid:22/Org/Repo.git",
        "ssh://git@example.invalid:22/org/Repo.git",
        "ssh://git@example.invalid:2222/Org/Repo.git",
        "https://example.invalid/Org/Repo.git",
    ] {
        git(&env, &project, &["config", "remote.upstream.url", wrong]);
        let output = env.run(&project, &["--project", "app", "verify"]);
        assert_eq!(output.status.code(), Some(1), "wrong URL passed: {wrong}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("URL mismatch"));
    }
}

#[test]
fn previous_expected_generation_is_refused_without_writes() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    let mut old = expected(&env, &project);
    old["format"] = "omd.expected/2".into();
    let before = std::fs::read(meta.join("state.toml")).unwrap();
    let output = env.run_raw(
        &project,
        &[
            "--project".into(),
            "app".into(),
            "--expected".into(),
            old.to_string(),
            "commit".into(),
            "unclean".into(),
            "a.md".into(),
            "--reason".into(),
            "old expected".into(),
        ],
    );
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(std::fs::read(meta.join("state.toml")).unwrap(), before);
}

#[test]
fn logical_registration_revision_invalidates_old_expected() {
    let env = Env::new();
    let old_url = "ssh://git@example.invalid:22/Org/Repo.git";
    let new_url = "ssh://git@example.invalid:2222/Org/Repo.git";
    let (project, meta) = fixture(&env, old_url);
    assert_ok(&register(&env, &project, &meta, Some(old_url)));
    let old_expected = expected(&env, &project);

    git(&env, &project, &["config", "remote.upstream.url", new_url]);
    assert_ok(&register(&env, &project, &meta, Some(new_url)));
    let after_registration = std::fs::read(meta.join("state.toml")).unwrap();
    let stale = env.run_raw(
        &project,
        &[
            "--project".into(),
            "app".into(),
            "--expected".into(),
            old_expected.to_string(),
            "commit".into(),
            "unclean".into(),
            "a.md".into(),
            "--reason".into(),
            "stale registration".into(),
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(
        std::fs::read(meta.join("state.toml")).unwrap(),
        after_registration
    );

    let fresh = expected(&env, &project);
    assert_ok(&env.run_raw(
        &project,
        &[
            "--project".into(),
            "app".into(),
            "--expected".into(),
            fresh.to_string(),
            "commit".into(),
            "unclean".into(),
            "a.md".into(),
            "--reason".into(),
            "fresh registration".into(),
        ],
    ));
}

#[test]
fn identity_change_blocks_stale_and_current_business_writes_before_publication() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    let old_expected = expected(&env, &project);

    assert_ok(&recognize(
        &env,
        &project,
        "ssh://git@example.invalid:2222/Org/Repo.git",
    ));
    let state_before = std::fs::read(meta.join("state.toml")).unwrap();
    let stale_args = vec![
        "--project".into(),
        "app".into(),
        "--expected".into(),
        old_expected.to_string(),
        "commit".into(),
        "commit".into(),
        "a.md".into(),
        "--reason".into(),
        "x".into(),
    ];
    let stale = env.run_raw(&project, &stale_args);
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(
        std::fs::read(meta.join("state.toml")).unwrap(),
        state_before
    );

    let fresh_expected = expected(&env, &project);
    let fresh_args = vec![
        "--project".into(),
        "app".into(),
        "--expected".into(),
        fresh_expected.to_string(),
        "commit".into(),
        "unclean".into(),
        "a.md".into(),
        "--reason".into(),
        "new mapping evidence".into(),
    ];
    assert_ok(&env.run_raw(&project, &fresh_args));

    let current_expected = expected(&env, &project);
    let current_state = std::fs::read(meta.join("state.toml")).unwrap();
    git(
        &env,
        &project,
        &[
            "config",
            "remote.upstream.url",
            "ssh://other@example.invalid:22/Org/Repo.git",
        ],
    );
    let changed_args = vec![
        "--project".into(),
        "app".into(),
        "--expected".into(),
        current_expected.to_string(),
        "commit".into(),
        "commit".into(),
        "a.md".into(),
        "--reason".into(),
        "x".into(),
    ];
    let changed = env.run_raw(&project, &changed_args);
    assert_eq!(changed.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&changed.stderr).contains("remote upstream URL mismatch"));
    assert_eq!(
        std::fs::read(meta.join("state.toml")).unwrap(),
        current_state
    );
}

#[test]
fn explicit_same_root_and_metadata_do_not_waive_registered_identity() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    git(
        &env,
        &project,
        &[
            "config",
            "remote.upstream.url",
            "https://wrong.invalid/repo",
        ],
    );

    let output = env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--json",
            "verify",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("URL mismatch"));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let evidence = value["data"]["expected"].to_string();
    let before = snapshot(&[&meta, &env.config]);
    let write = env.run_raw(
        &project,
        &[
            "--root".into(),
            project.to_string_lossy().into_owned(),
            "--meta".into(),
            meta.to_string_lossy().into_owned(),
            "--expected".into(),
            evidence,
            "commit".into(),
            "unclean".into(),
            "a.md".into(),
            "--reason".into(),
            "must not bypass identity".into(),
        ],
    );
    assert_eq!(write.status.code(), Some(3));
    assert_eq!(snapshot(&[&meta, &env.config]), before);
}

#[test]
fn raw_remote_value_preserves_carriage_return() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    git(
        &env,
        &project,
        &["config", "remote.upstream.url", &format!("{exact}\r")],
    );

    let output = env.run(&project, &["--project", "app", "verify"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("URL mismatch"));
}

#[test]
fn project_mutations_require_current_caller_evidence() {
    let env = Env::new();
    let first = "ssh://git@example.invalid:22/Org/Repo.git";
    let second = "ssh://git@example.invalid:2222/Org/Repo.git";
    let third = "https://example.invalid/Org/Repo.git";
    let (project, meta) = fixture(&env, first);
    assert_ok(&register(&env, &project, &meta, Some(first)));
    let stale = expected(&env, &project);

    git(&env, &project, &["config", "remote.upstream.url", second]);
    assert_ok(&register(&env, &project, &meta, Some(second)));
    let evidence_file = env.root.path().join("stale-expected.json");
    std::fs::write(&evidence_file, stale.to_string()).unwrap();
    let before = snapshot(&[&meta, &env.config]);

    for args in [
        vec![
            "--expected".to_string(),
            evidence_file.to_string_lossy().into_owned(),
            "project".into(),
            "register".into(),
            "app".into(),
            project.to_string_lossy().into_owned(),
            meta.to_string_lossy().into_owned(),
            "--git-remote".into(),
            "upstream".into(),
            "--git-remote-url".into(),
            third.into(),
        ],
        vec![
            "--expected".to_string(),
            evidence_file.to_string_lossy().into_owned(),
            "project".into(),
            "recognize-remote".into(),
            "app".into(),
            "--git-remote-url".into(),
            third.into(),
        ],
    ] {
        let output = env.run_raw(&project, &args);
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(snapshot(&[&meta, &env.config]), before);
    }

    for args in [
        vec![
            "project".to_string(),
            "register".into(),
            "app".into(),
            project.to_string_lossy().into_owned(),
            meta.to_string_lossy().into_owned(),
            "--git-remote".into(),
            "upstream".into(),
            "--git-remote-url".into(),
            third.into(),
        ],
        vec![
            "project".to_string(),
            "recognize-remote".into(),
            "app".into(),
            "--git-remote-url".into(),
            third.into(),
        ],
    ] {
        let output = env.run_raw(&project, &args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(snapshot(&[&meta, &env.config]), before);
    }

    let malformed = env.root.path().join("malformed-expected.json");
    std::fs::write(&malformed, "{").unwrap();
    for args in [
        vec![
            "--expected".to_string(),
            malformed.to_string_lossy().into_owned(),
            "project".into(),
            "register".into(),
            "app".into(),
            project.to_string_lossy().into_owned(),
            meta.to_string_lossy().into_owned(),
            "--git-remote".into(),
            "upstream".into(),
            "--git-remote-url".into(),
            third.into(),
        ],
        vec![
            "--expected".to_string(),
            malformed.to_string_lossy().into_owned(),
            "project".into(),
            "recognize-remote".into(),
            "app".into(),
            "--git-remote-url".into(),
            third.into(),
        ],
    ] {
        let output = env.run_raw(&project, &args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(snapshot(&[&meta, &env.config]), before);
    }
}

#[test]
fn fresh_target_alias_evidence_updates_only_that_alias() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let approved = "https://alternative.invalid/repo";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));

    let app_evidence = expected(&env, &project).to_string();
    assert_ok(&env.run(
        &project,
        &[
            "--expected",
            &app_evidence,
            "project",
            "register",
            "aux",
            project.to_str().unwrap(),
            meta.to_str().unwrap(),
            "--git-remote",
            "upstream",
            "--git-remote-url",
            exact,
        ],
    ));
    assert_ok(&recognize(&env, &project, approved));

    let observed = env.run(&project, &["--project", "aux", "--json", "verify"]);
    assert_ok(&observed);
    let value: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    let aux_evidence = value["data"]["expected"].to_string();
    assert_ok(&env.run(
        &project,
        &[
            "--expected",
            &aux_evidence,
            "project",
            "register",
            "aux",
            project.to_str().unwrap(),
            meta.to_str().unwrap(),
            "--git-remote",
            "upstream",
            "--git-remote-url",
            exact,
        ],
    ));
}

#[test]
fn bound_store_rejects_mapping_changed_after_observation() {
    let approved = "https://alternative.invalid/repo";
    let changed = "ssh://git@example.invalid:2222/Org/NewRepo.git";
    if std::env::var_os("OMD_DIRECT_STORE_CHILD").is_some() {
        let project = PathBuf::from(std::env::var_os("OMD_DIRECT_PROJECT").unwrap());
        let meta = PathBuf::from(std::env::var_os("OMD_DIRECT_META").unwrap());
        let context = projects::select_context(&project, None, None, Some("app"), false).unwrap();
        let mut store = Store::open_existing(&context.metadata_root).unwrap();
        store.bind_context(
            context.instance.clone(),
            context.mapping_revision,
            context.project_root.clone(),
            context.alias.clone(),
            project.clone(),
            BTreeSet::new(),
        );
        let evidence =
            Expected::observe(&store, context.instance.clone(), context.mapping_revision).unwrap();

        projects::recognize_remote(&project, "app", approved, &evidence).unwrap();
        let authority_after_recognition = snapshot(&[&meta]);
        let publication = store.state().publication;
        let error = store
            .register_project(
                "app",
                Some(RemoteIdentity {
                    name: "upstream".into(),
                    url: changed.into(),
                }),
                &evidence,
            )
            .unwrap_err();
        assert!(matches!(error, StoreError::Conflict(_)), "{error:?}");
        assert_eq!(store.state().publication, publication);
        assert_eq!(snapshot(&[&meta]), authority_after_recognition);
        drop(store);

        let mut ordinary = Store::open_existing(&context.metadata_root).unwrap();
        ordinary.bind_context(
            context.instance,
            context.mapping_revision,
            context.project_root,
            context.alias,
            project,
            BTreeSet::new(),
        );
        let error = ordinary.check_expected(&evidence).unwrap_err();
        assert!(matches!(error, StoreError::Conflict(_)), "{error:?}");
        assert_eq!(snapshot(&[&meta]), authority_after_recognition);
        return;
    }

    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bound_store_rejects_mapping_changed_after_observation",
            "--nocapture",
        ])
        .env("OMD_DIRECT_STORE_CHILD", "1")
        .env("OMD_DIRECT_PROJECT", &project)
        .env("OMD_DIRECT_META", &meta)
        .env("HOME", &env.home)
        .env("OMD_CONFIG_PATH", &env.config)
        .env("OMD_CACHE_PATH", &env.cache)
        .output()
        .unwrap();
    assert_ok(&output);
}

#[test]
fn project_mutation_lock_conflict_keeps_typed_exit_and_authority() {
    let env = Env::new();
    let exact = "ssh://git@example.invalid:22/Org/Repo.git";
    let (project, meta) = fixture(&env, exact);
    assert_ok(&register(&env, &project, &meta, Some(exact)));
    let evidence = expected(&env, &project).to_string();

    let mut holder = Store::open_existing(&meta).unwrap();
    holder.lock().unwrap();
    let before = snapshot(&[&meta, &env.config]);
    let output = env.run(
        &project,
        &[
            "--expected",
            &evidence,
            "project",
            "recognize-remote",
            "app",
            "--git-remote-url",
            "https://alternative.invalid/repo",
        ],
    );

    assert_eq!(output.status.code(), Some(4));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("lock conflict"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(snapshot(&[&meta, &env.config]), before);
}

#[test]
fn metadata_repair_does_not_require_command_execution() {
    let env = Env::new();
    let first = "ssh://git@example.invalid:22/Org/Repo.git";
    let second = "ssh://git@example.invalid:2222/Org/Repo.git";
    let project = env.root.path().join("command-project");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&project).unwrap();
    git(&env, &project, &["init", "-q"]);
    git(&env, &project, &["config", "remote.upstream.url", first]);
    let count = project.join("count");
    let script = project.join("source.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nn=0\n[ ! -f count ] || n=$(cat count)\necho $((n+1)) > count\nprintf 'command body\\n'\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    let args = serde_json::to_string(&vec!["source.sh"]).unwrap();
    assert_ok(&env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            args.as_str(),
            "commit",
            "init",
            "virtual.md",
        ],
    ));
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "1\n");
    assert_ok(&register(&env, &project, &meta, Some(first)));

    git(&env, &project, &["config", "remote.upstream.url", second]);
    let observed = env.run(&project, &["--project", "app", "--json", "verify"]);
    assert_eq!(observed.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    assert!(
        value["data"]["expected"]["source_versions"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let evidence = value["data"]["expected"].to_string();
    let repair = env.run_raw(
        &project,
        &[
            "--expected".into(),
            evidence,
            "project".into(),
            "register".into(),
            "app".into(),
            project.to_string_lossy().into_owned(),
            meta.to_string_lossy().into_owned(),
            "--git-remote".into(),
            "upstream".into(),
            "--git-remote-url".into(),
            second.into(),
        ],
    );
    assert_ok(&repair);
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "1\n");
    let after = env.run(&project, &["--project", "app", "verify"]);
    assert_eq!(after.status.code(), Some(1));
    let text = String::from_utf8_lossy(&after.stdout);
    assert!(text.contains("command source, not run"), "{text}");
    assert!(!text.contains("URL mismatch"), "{text}");
}

#[test]
fn moved_mapping_is_diagnosable_and_repairable_with_target_evidence() {
    for metadata_only in [false, true] {
        let env = Env::new();
        let exact = "ssh://git@example.invalid:22/Org/Repo.git";
        let (project, meta) = fixture(&env, exact);
        assert_ok(&register(&env, &project, &meta, Some(exact)));
        let old_registration = state_registration(&meta);
        let old_registration_bytes =
            std::fs::read(meta.join(format!("registrations/{old_registration}.toml"))).unwrap();

        let (moved, moved_meta) = if metadata_only {
            let moved_meta = project.join("moved-meta");
            std::fs::rename(&meta, &moved_meta).unwrap();
            (project.clone(), moved_meta)
        } else {
            let moved = env.root.path().join("moved-project");
            std::fs::rename(&project, &moved).unwrap();
            let moved_meta = moved.join(".omd");
            (moved, moved_meta)
        };
        let observed = env.run(
            &moved,
            &[
                "--root",
                moved.to_str().unwrap(),
                "--meta",
                moved_meta.to_str().unwrap(),
                "--json",
                "verify",
            ],
        );
        assert_eq!(observed.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&observed.stdout).contains("mapping app is missing"));
        let value: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
        let evidence = value["data"]["expected"].to_string();

        let repair = env.run_raw(
            &moved,
            &[
                "--expected".into(),
                evidence,
                "project".into(),
                "register".into(),
                "app".into(),
                moved.to_string_lossy().into_owned(),
                moved_meta.to_string_lossy().into_owned(),
                "--git-remote".into(),
                "upstream".into(),
                "--git-remote-url".into(),
                exact.into(),
            ],
        );
        assert_ok(&repair);
        assert_ok(&env.run(&moved, &["--project", "app", "verify"]));
        assert_eq!(state_registration(&moved_meta), old_registration);
        assert_eq!(
            std::fs::read(moved_meta.join(format!("registrations/{old_registration}.toml")))
                .unwrap(),
            old_registration_bytes
        );
    }
}
