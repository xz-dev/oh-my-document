use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        Self {
            config: root.path().join("config"),
            cache: root.path().join("cache"),
            root,
        }
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(omd())
            .args(args)
            .current_dir(cwd)
            .env("HOME", self.root.path().join("home"))
            .env("OMD_CONFIG_PATH", &self.config)
            .env("OMD_CACHE_PATH", &self.cache)
            .output()
            .unwrap()
    }
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn expected(env: &Env, root: &Path, meta: &Path, extra: &[&str]) -> String {
    let mut args = vec![
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--meta",
        meta.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    args.push("verify");
    let output = env.run(root, &args);
    let value = json(&output);
    serde_json::to_string(&value["data"]["expected"]).unwrap()
}

fn init(env: &Env, root: &Path, meta: &Path, path: &str, body: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join(path), body).unwrap();
    let output = env.run(
        root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "init",
            path,
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else if path.file_name().and_then(|v| v.to_str()) != Some("write.lock") {
                    out.push((
                        path.strip_prefix(base).unwrap().into(),
                        std::fs::read(path).unwrap(),
                    ));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn command_fixture(env: &Env, root: &Path, meta: &Path) -> String {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("body"), "alpha\n").unwrap();
    let script = root.join("source.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nn=0\n[ ! -f count ] || n=$(cat count)\necho $((n+1)) > count\nif [ -f fail ]; then printf partial; exit 7; fi\ncat body\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let source_ref = serde_json::to_string(&vec!["source.sh"]).unwrap();
    let output = env.run(
        root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            &source_ref,
            "commit",
            "init",
            "virtual.md",
        ],
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(root.join("count")).unwrap(), "1\n");
    source_ref
}

fn observe(env: &Env, root: &Path, meta: &Path, extra: &[&str]) -> (Output, serde_json::Value) {
    let mut args = vec![
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--meta",
        meta.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    args.push("verify");
    let output = env.run(root, &args);
    let value = json(&output);
    (output, value)
}

fn metadata_expected(env: &Env, cwd: &Path, root: &Path, meta: &Path) -> String {
    let output = env.run(
        cwd,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "verify",
        ],
    );
    json(&output)["data"]["expected"].to_string()
}

fn command_update(
    env: &Env,
    root: &Path,
    meta: &Path,
    source_ref: &str,
    evidence: &serde_json::Value,
) -> Output {
    env.run(
        root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            source_ref,
            "--expected",
            &evidence.to_string(),
            "commit",
            "commit",
            "virtual.md",
            "--reason",
            "credential transition",
        ],
    )
}

#[test]
fn file_observation_rejects_hash_changed_without_matching_version() {
    let env = Env::new();
    let root = env.root.path().join("inconsistent-file-tuple");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "body", "alpha\n");
    let (_, value) = observe(&env, &root, &meta, &[]);
    let mut evidence = value["data"]["expected"].clone();
    let node = evidence["source_versions"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    std::fs::write(root.join("body"), "beta\n").unwrap();
    evidence["source_hashes"][&node] =
        omd::records::version::SourceVersion::content_sha256(b"beta\n").into();
    let before = snapshot(&meta);

    let update = env.run(
        &root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--expected",
            &evidence.to_string(),
            "commit",
            "commit",
            "body",
            "--reason",
            "inconsistent tuple",
        ],
    );
    assert_eq!(update.status.code(), Some(3));
    assert_eq!(json(&update)["diagnostics"][0]["kind"], "version_conflict");
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn command_observation_rejects_content_bytes_not_matching_version_hash() {
    let env = Env::new();
    let root = env.root.path().join("corrupt-command-tuple");
    let meta = root.join(".omd");
    let source_ref = command_fixture(&env, &root, &meta);
    let (observed, value) = observe(&env, &root, &meta, &["--run-command=true"]);
    assert!(observed.status.success(), "observation={value}");
    let evidence = value["data"]["expected"].clone();
    let node = evidence["source_versions"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let hash = evidence["source_hashes"][node].as_str().unwrap();
    std::fs::write(meta.join(format!("content/{hash}")), "corrupt\n").unwrap();
    let before = snapshot(&meta);

    let update = command_update(&env, &root, &meta, &source_ref, &evidence);
    assert_eq!(
        update.status.code(),
        Some(3),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );
    assert_eq!(json(&update)["diagnostics"][0]["kind"], "version_conflict");
    assert_eq!(snapshot(&meta), before);
    assert_eq!(std::fs::read_to_string(root.join("count")).unwrap(), "2\n");
}

#[test]
fn successful_changed_command_observation_commits_exact_bytes_without_rerun() {
    let env = Env::new();
    let root = env.root.path().join("changed-command");
    let meta = root.join(".omd");
    let source_ref = command_fixture(&env, &root, &meta);
    std::fs::write(root.join("body"), "beta\n").unwrap();

    let (observed, value) = observe(&env, &root, &meta, &["--run-command=true"]);
    assert_eq!(observed.status.code(), Some(1), "observation={value}");
    let evidence = value["data"]["expected"].clone();
    let node = evidence["tips"].as_object().unwrap().keys().next().unwrap();
    let basis = evidence["basis_versions"][node].as_str().unwrap();
    let acquired = evidence["acquisition_versions"][node].as_str().unwrap();
    assert_ne!(basis, acquired);
    let version: toml::Value = toml::from_str(
        &std::fs::read_to_string(meta.join(format!("versions/{acquired}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        version["sha256"].as_str(),
        evidence["source_hashes"][node].as_str()
    );
    let hash = version["sha256"].as_str().unwrap();
    assert_eq!(
        std::fs::read(meta.join(format!("content/{hash}"))).unwrap(),
        b"beta\n"
    );

    let update = command_update(&env, &root, &meta, &source_ref, &evidence);
    assert_eq!(
        update.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );
    assert_eq!(std::fs::read_to_string(root.join("count")).unwrap(), "2\n");
    let commit_id = json(&update)["data"]["commit"]
        .as_str()
        .unwrap()
        .to_string();
    let commit: toml::Value = toml::from_str(
        &std::fs::read_to_string(meta.join(format!("commits/{commit_id}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(commit["content_ref"].as_str(), Some(acquired));
}

#[test]
fn denied_command_observation_cannot_authorize_write() {
    let env = Env::new();
    let root = env.root.path().join("denied-command");
    let meta = root.join(".omd");
    let source_ref = command_fixture(&env, &root, &meta);

    let (observed, value) = observe(&env, &root, &meta, &[]);
    assert_eq!(observed.status.code(), Some(1), "observation={value}");
    let evidence = value["data"]["expected"].clone();
    let before = snapshot(&meta);
    let update = command_update(&env, &root, &meta, &source_ref, &evidence);
    assert_eq!(update.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);
    assert_eq!(std::fs::read_to_string(root.join("count")).unwrap(), "1\n");

    let checked = env.run(
        &root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "check",
        ],
    );
    let checked_value = json(&checked);
    let checked_evidence = checked_value["data"]["check"]["expected"].clone();
    assert!(
        checked_evidence["source_versions"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let update = command_update(&env, &root, &meta, &source_ref, &checked_evidence);
    assert_eq!(update.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);
    assert_eq!(std::fs::read_to_string(root.join("count")).unwrap(), "1\n");
}

#[test]
fn failed_command_observation_discards_partial_stdout_and_cannot_authorize_write() {
    let env = Env::new();
    let root = env.root.path().join("failed-command");
    let meta = root.join(".omd");
    let source_ref = command_fixture(&env, &root, &meta);
    std::fs::write(root.join("fail"), "yes").unwrap();

    let (observed, value) = observe(&env, &root, &meta, &["--run-command=true"]);
    assert_eq!(observed.status.code(), Some(1), "observation={value}");
    let evidence = value["data"]["expected"].clone();
    let before = snapshot(&meta);
    let update = command_update(&env, &root, &meta, &source_ref, &evidence);
    assert_eq!(update.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);
    assert_eq!(std::fs::read_to_string(root.join("count")).unwrap(), "2\n");
}

#[test]
fn missing_tip_evidence_cannot_weaken_existing_object_write_preconditions() {
    let env = Env::new();
    let root = env.root.path().join("missing-tip");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "body", "alpha\n");
    let (_, value) = observe(&env, &root, &meta, &[]);
    let mut evidence = value["data"]["expected"].clone();
    evidence.as_object_mut().unwrap().remove("tips");
    let before = snapshot(&meta);

    let update = env.run(
        &root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--expected",
            &evidence.to_string(),
            "commit",
            "commit",
            "body",
            "--reason",
            "missing tip",
        ],
    );
    assert!(matches!(update.status.code(), Some(2 | 3)));
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn changed_file_observation_reuses_exact_acquired_version_despite_unrelated_failure() {
    let env = Env::new();
    let root = env.root.path().join("changed-file");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "a.md", "alpha\n");
    init(&env, &root, &meta, "b.md", "stable\n");
    let before_unclean = expected(&env, &root, &meta, &[]);
    let unclean = env.run(
        &root,
        &[
            "--expected",
            &before_unclean,
            "commit",
            "unclean",
            "b.md",
            "--reason",
            "unrelated obligation",
        ],
    );
    assert!(unclean.status.success());
    std::fs::write(root.join("a.md"), "beta\n").unwrap();

    let (observed, value) = observe(&env, &root, &meta, &[]);
    assert_eq!(observed.status.code(), Some(1), "observation={value}");
    let evidence = value["data"]["expected"].clone();
    let a_node = evidence["tips"]
        .as_object()
        .unwrap()
        .keys()
        .find(|node| {
            let state: omd::records::store::State =
                toml::from_str(&std::fs::read_to_string(meta.join("state.toml")).unwrap()).unwrap();
            state
                .locations
                .get(*node)
                .is_some_and(|path| path == "a.md")
        })
        .unwrap()
        .clone();
    let acquired = evidence["acquisition_versions"][&a_node]
        .as_str()
        .unwrap()
        .to_string();
    let hash = evidence["source_hashes"][&a_node]
        .as_str()
        .unwrap()
        .to_string();
    let version: omd::records::version::SourceVersion = toml::from_str(
        &std::fs::read_to_string(meta.join(format!("versions/{acquired}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(version.sha256, hash);
    assert_eq!(
        std::fs::read(meta.join(format!("content/{hash}"))).unwrap(),
        b"beta\n"
    );

    let update = env.run(
        &root,
        &[
            "--json",
            "--expected",
            &evidence.to_string(),
            "commit",
            "commit",
            "a.md",
            "--reason",
            "selected source succeeded",
        ],
    );
    assert_eq!(
        update.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );
    let commit_id = json(&update)["data"]["commit"]
        .as_str()
        .unwrap()
        .to_string();
    let commit: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(meta.join(format!("commits/{commit_id}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(commit.content_ref, acquired);
}

#[test]
fn affected_tip_and_registration_evidence_are_required_under_lock() {
    let env = Env::new();
    let root = env.root.path().join("coverage");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "body", "alpha\n");
    let state_path = meta.join("state.toml");
    let mut state: omd::records::store::State =
        toml::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    state
        .registrations
        .insert("app".into(), "registration-revision".into());
    std::fs::write(&state_path, toml::to_string(&state).unwrap()).unwrap();

    let (_, value) = observe(&env, &root, &meta, &[]);
    let expected = value["data"]["expected"].clone();
    let before = snapshot(&meta);

    let mut missing_tip = expected.clone();
    let tip = missing_tip["tips"]
        .as_object_mut()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    missing_tip["tips"].as_object_mut().unwrap().remove(&tip);
    let rejected = env.run(
        &root,
        &[
            "--json",
            "--expected",
            &missing_tip.to_string(),
            "commit",
            "unclean",
            "body",
            "--reason",
            "missing affected tip",
        ],
    );
    assert_eq!(rejected.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);

    let mut missing_registration = expected;
    missing_registration["registrations"]
        .as_object_mut()
        .unwrap()
        .remove("app");
    let rejected = env.run(
        &root,
        &[
            "--json",
            "--expected",
            &missing_registration.to_string(),
            "commit",
            "unclean",
            "body",
            "--reason",
            "missing registration",
        ],
    );
    assert_eq!(rejected.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn observed_existing_update_succeeds_and_stale_publication_fails_without_mutation() {
    let env = Env::new();
    let root = env.root.path().join("project");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "a.md", "one\n");
    let evidence = expected(&env, &root, &meta, &[]);
    let evidence_json: serde_json::Value = serde_json::from_str(&evidence).unwrap();
    assert!(
        evidence_json["instance"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert!(
        evidence_json["project_id"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert!(
        evidence_json["store_id"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert_eq!(evidence_json["publication"], 1);
    assert!(!evidence_json["tips"].as_object().unwrap().is_empty());
    assert!(
        !evidence_json["source_versions"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert!(
        !evidence_json["acquisition_versions"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert!(
        !evidence_json["source_hashes"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert!(evidence_json["registrations"].is_object());
    let evidence_file = env.root.path().join("expected.json");
    std::fs::write(&evidence_file, &evidence).unwrap();

    let ok = env.run(
        &root,
        &[
            "--json",
            "--expected",
            evidence_file.to_str().unwrap(),
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "review",
        ],
    );
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    let before = snapshot(&meta);
    let stale = env.run(
        &root,
        &[
            "--json",
            "--expected",
            &evidence,
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "stale",
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn mapping_revision_and_instance_bind_observation() {
    let env = Env::new();
    let root = env.root.path().join("a");
    let moved = env.root.path().join("b");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "a.md", "same\n");
    let initial_evidence = metadata_expected(&env, env.root.path(), &root, &meta);
    let register = env.run(
        env.root.path(),
        &[
            "--expected",
            initial_evidence.as_str(),
            "project",
            "register",
            "app",
            root.to_str().unwrap(),
            meta.to_str().unwrap(),
        ],
    );
    assert!(register.status.success());
    let observed = env.run(&root, &["--json", "--project", "app", "verify"]);
    assert!(
        observed.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&observed.stdout),
        String::from_utf8_lossy(&observed.stderr)
    );
    let observed_json = json(&observed);
    let old_json = observed_json["data"]["expected"].clone();
    let old = serde_json::to_string(&old_json).unwrap();
    assert_eq!(
        old_json["mapping_revision"], 1,
        "observation={observed_json}"
    );
    assert!(old_json["registrations"].is_object());

    std::fs::rename(&root, &moved).unwrap();
    let moved_meta = moved.join(".omd");
    let moved_evidence = metadata_expected(&env, env.root.path(), &moved, &moved_meta);
    let register = env.run(
        env.root.path(),
        &[
            "--expected",
            moved_evidence.as_str(),
            "project",
            "register",
            "app",
            moved.to_str().unwrap(),
            moved_meta.to_str().unwrap(),
        ],
    );
    assert!(
        register.status.success(),
        "{}",
        String::from_utf8_lossy(&register.stderr)
    );
    let before = snapshot(&moved_meta);
    let stale = env.run(
        &moved,
        &[
            "--json",
            "--project",
            "app",
            "--expected",
            &old,
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "old placement",
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(snapshot(&moved_meta), before);

    let fresh_output = env.run(&moved, &["--json", "--project", "app", "verify"]);
    let fresh = serde_json::to_string(&json(&fresh_output)["data"]["expected"]).unwrap();
    let ok = env.run(
        &moved,
        &[
            "--json",
            "--project",
            "app",
            "--expected",
            &fresh,
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "new placement",
        ],
    );
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

#[test]
fn another_metadata_instance_rejects_first_instance_observation() {
    let env = Env::new();
    let root_a = env.root.path().join("a");
    let meta_a = root_a.join(".omd");
    init(&env, &root_a, &meta_a, "a.md", "same\n");
    let evidence = expected(&env, &root_a, &meta_a, &[]);

    let root_b = env.root.path().join("b");
    std::fs::create_dir_all(&root_b).unwrap();
    std::fs::write(root_b.join("a.md"), "same\n").unwrap();
    let meta_b = root_b.join(".omd");
    copy_dir::copy_dir(&meta_a, &meta_b);
    let copy_evidence = expected(&env, &root_b, &meta_b, &[]);
    let activate = env.run(
        &root_b,
        &[
            "--root",
            root_b.to_str().unwrap(),
            "--meta",
            meta_b.to_str().unwrap(),
            "--expected",
            &copy_evidence,
            "activate",
        ],
    );
    assert!(activate.status.success());
    let before = snapshot(&meta_b);
    let stale = env.run(
        &root_b,
        &[
            "--root",
            root_b.to_str().unwrap(),
            "--meta",
            meta_b.to_str().unwrap(),
            "--expected",
            &evidence,
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "wrong instance",
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(snapshot(&meta_b), before);
}

#[test]
fn file_change_after_observation_refuses_write_and_keeps_history() {
    let env = Env::new();
    let root = env.root.path().join("project");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "a.md", "one\n");
    let evidence = expected(&env, &root, &meta, &[]);
    std::fs::write(root.join("a.md"), "two\n").unwrap();
    let before = snapshot(&meta);

    let stale = env.run(
        &root,
        &[
            "--json",
            "--expected",
            &evidence,
            "commit",
            "unclean",
            "a.md",
            "--reason",
            "stale bytes",
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn command_observation_is_persisted_and_reused_without_rerun() {
    let env = Env::new();
    let root = env.root.path().join("project");
    let meta = root.join(".omd");
    std::fs::create_dir_all(&root).unwrap();
    let counter = root.join("count");
    let script = root.join("source.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nn=0\n[ -f '{}' ] && n=$(cat '{}')\nn=$((n+1))\nprintf '%s' \"$n\" > '{}'\nprintf 'body\\n'\n",
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
    let source_ref = serde_json::to_string(&vec![script.to_string_lossy().to_string()]).unwrap();
    let init = env.run(
        &root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            &source_ref,
            "commit",
            "init",
            "virtual.md",
        ],
    );
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "1");

    let evidence = expected(&env, &root, &meta, &["--run-command=true"]);
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "2");
    let missing = env.run(
        &root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            &source_ref,
            "commit",
            "commit",
            "virtual.md",
            "--reason",
            "missing",
        ],
    );
    assert_eq!(missing.status.code(), Some(3));
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "2");

    let update = env.run(
        &root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            &source_ref,
            "--expected",
            &evidence,
            "commit",
            "commit",
            "virtual.md",
            "--reason",
            "reuse",
        ],
    );
    assert_eq!(
        update.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "2");

    let stale = env.run(
        &root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            &source_ref,
            "--expected",
            &evidence,
            "commit",
            "commit",
            "virtual.md",
            "--reason",
            "stale",
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "2");
}

#[test]
fn reindex_is_instance_scoped_rebuildable_and_never_executes_sources() {
    let env = Env::new();
    let root_a = env.root.path().join("a");
    let meta_a = root_a.join(".omd");
    std::fs::create_dir_all(&root_a).unwrap();
    let counter = root_a.join("count");
    let script = root_a.join("source.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nn=0\n[ -f '{}' ] && n=$(cat '{}')\nn=$((n+1))\nprintf '%s' \"$n\" > '{}'\nprintf 'body\\n'\n", counter.display(), counter.display(), counter.display()),
    ).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let source_ref = serde_json::to_string(&vec![script.to_string_lossy().to_string()]).unwrap();
    let init = env.run(
        &root_a,
        &[
            "--root",
            root_a.to_str().unwrap(),
            "--meta",
            meta_a.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            &source_ref,
            "commit",
            "init",
            "virtual.md",
        ],
    );
    assert!(init.status.success());
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "1");

    let first = env.run(
        &root_a,
        &[
            "--json",
            "--root",
            root_a.to_str().unwrap(),
            "--meta",
            meta_a.to_str().unwrap(),
            "reindex",
        ],
    );
    assert!(first.status.success());
    let first_json = json(&first);
    let first_path = PathBuf::from(first_json["data"]["cache_file"].as_str().unwrap());
    let first_bytes = std::fs::read(&first_path).unwrap();
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "1");
    std::fs::remove_dir_all(&env.cache).unwrap();
    let rebuilt = env.run(
        &root_a,
        &[
            "--json",
            "--root",
            root_a.to_str().unwrap(),
            "--meta",
            meta_a.to_str().unwrap(),
            "reindex",
        ],
    );
    assert!(rebuilt.status.success());
    let rebuilt_json = json(&rebuilt);
    let rebuilt_path = PathBuf::from(rebuilt_json["data"]["cache_file"].as_str().unwrap());
    assert_eq!(first_bytes, std::fs::read(&rebuilt_path).unwrap());
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "1");

    let root_b = env.root.path().join("b");
    let meta_b = root_b.join(".omd");
    std::fs::create_dir_all(&root_b).unwrap();
    copy_dir::copy_dir(&meta_a, &meta_b);
    let second = env.run(
        &root_b,
        &[
            "--json",
            "--root",
            root_b.to_str().unwrap(),
            "--meta",
            meta_b.to_str().unwrap(),
            "reindex",
        ],
    );
    assert!(second.status.success());
    let second_json = json(&second);
    let second_path = PathBuf::from(second_json["data"]["cache_file"].as_str().unwrap());
    assert_ne!(rebuilt_path.parent(), second_path.parent());
    assert_eq!(first_bytes, std::fs::read(second_path).unwrap());
    assert_eq!(std::fs::read_to_string(&counter).unwrap(), "1");
}

#[test]
fn library_version_and_binding_writers_reject_nonportable_descriptors() {
    let env = Env::new();
    let root = env.root.path().join("portable-writers");
    let meta = root.join(".omd");
    std::fs::create_dir_all(&root).unwrap();
    let mut store = omd::records::store::Store::open(&meta).unwrap();
    let absolute = root.join("source.md").to_string_lossy().into_owned();
    let descriptor = omd::sources::SourceDescriptor::File {
        project: "root".into(),
        path: absolute,
    };
    let version = omd::records::version::SourceVersion::new(
        omd::records::ids::Id128([7; 16]),
        b"abc",
        descriptor.clone(),
        Some("utf-8".into()),
    );
    let before = snapshot(&meta);
    assert!(store.persist_observation(&version, b"abc").is_err());
    assert_eq!(snapshot(&meta), before);

    let binding = omd::records::binding::Binding {
        format: "omd.binding/2".into(),
        id: "1".repeat(32),
        version_id: "2".repeat(32),
        recovery: descriptor,
        seq: 1,
        affected: vec!["3".repeat(64)],
    };
    assert!(binding.validate(&binding.id, &binding.version_id).is_err());
}

#[test]
fn invalid_encoding_config_rejects_before_command_and_publication() {
    let env = Env::new();
    let root = env.root.path().join("configured-command");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "doc.md", "abc");
    let script = root.join("source.sh");
    std::fs::write(&script, "#!/bin/sh\necho run >> count\nprintf abc\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(
        meta.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"not-an-encoding\"\n",
    )
    .unwrap();
    let before = snapshot(&meta);
    let output = env.run(
        &root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--source-type",
            "command",
            "--executable",
            "/bin/sh",
            "--args-json",
            "[\"source.sh\"]",
            "init",
            "virtual.md",
        ],
    );
    assert!(!output.status.success());
    assert!(!root.join("count").exists());
    assert_eq!(snapshot(&meta), before);

    std::fs::write(meta.join("omd.toml"), "this is not valid TOML = [").unwrap();
    std::fs::write(root.join("other.md"), "abc").unwrap();
    let before = snapshot(&meta);
    let output = env.run(
        &root,
        &[
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "init",
            "other.md",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn library_collect_rejects_unknown_encoding_before_command_execution() {
    let env = Env::new();
    let root = env.root.path().join("library-command");
    let meta = root.join(".omd");
    init(&env, &root, &meta, "doc.md", "abc");
    let before = snapshot(&meta);
    let command = omd::sources::SourceDescriptor::Command {
        executable: "/bin/sh".into(),
        args: vec!["-c".into(), "echo run >> library-count; printf abc".into()],
    };
    let invalid = omd::sources::collect(&command, &root, &root, true, Some("not-an-encoding"));
    assert!(matches!(invalid, Err(omd::sources::SourceError::Encoding)));
    assert!(!root.join("library-count").exists());
    assert_eq!(snapshot(&meta), before);

    let valid = omd::sources::collect(&command, &root, &root, true, Some("utf-8")).unwrap();
    assert_eq!(valid.bytes, b"abc");
    assert_eq!(
        std::fs::read_to_string(root.join("library-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn library_commit_source_normalizes_absolute_recovery_before_publication() {
    let env = Env::new();
    let root = env.root.path().join("library-path");
    let meta = root.join(".omd");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("alternate.md"), "abc").unwrap();
    std::fs::write(root.join("new.md"), "abc").unwrap();
    let expected = omd::records::store::Expected::default();
    let recovery = omd::sources::SourceDescriptor::File {
        project: "root".into(),
        path: root.join("alternate.md").to_string_lossy().into_owned(),
    };
    let observation = omd::sources::collect(&recovery, &root, &root, true, Some("utf-8")).unwrap();
    let mut store = omd::records::store::Store::open(&meta).unwrap();
    store.bind_context(
        String::new(),
        expected.mapping_revision,
        root.clone(),
        None,
        root.clone(),
        Default::default(),
    );
    let payload = serde_json::json!({"path":"new.md"})
        .as_object()
        .unwrap()
        .clone();
    let commit = omd::records::pipeline::commit_source(
        &mut store,
        &mut omd::records::store::NoProbe,
        &omd::records::time::OsRng,
        &omd::records::time::SystemClock,
        "file:new.md",
        omd::sources::SourceDescriptor::File {
            project: "root".into(),
            path: "new.md".into(),
        },
        recovery,
        Some(observation),
        None,
        omd::records::commit::CommitKind::Init,
        payload,
        &expected,
    )
    .unwrap();
    let commit = store.read_commit(&commit).unwrap();
    let version = store.read_version(&commit.content_ref).unwrap();
    assert_eq!(
        version.recovery,
        omd::sources::SourceDescriptor::File {
            project: "root".into(),
            path: "alternate.md".into(),
        }
    );
}

mod copy_dir {
    use std::path::Path;
    pub fn copy_dir(source: &Path, target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let dest = target.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &dest);
            } else {
                std::fs::copy(entry.path(), dest).unwrap();
            }
        }
    }
}
