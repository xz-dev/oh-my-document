mod common;

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
        let config = root.path().join("config");
        let cache = root.path().join("cache");
        std::fs::create_dir_all(&config).unwrap();
        Self {
            root,
            config,
            cache,
        }
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(omd())
            .args(common::with_expected(
                &omd(),
                cwd,
                args,
                None,
                Some(&self.root.path().join("home")),
                Some(&self.config),
                Some(&self.cache),
            ))
            .current_dir(cwd)
            .env("HOME", self.root.path().join("home"))
            .env("OMD_CONFIG_PATH", &self.config)
            .env("OMD_CACHE_PATH", &self.cache)
            .output()
            .unwrap()
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

fn init_range(env: &Env, root: &Path, meta: &Path, body: &str) -> String {
    init_named_range(env, root, meta, "a.md", body)
}

fn init_named_range(env: &Env, root: &Path, meta: &Path, path: &str, body: &str) -> String {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join(path), body).unwrap();
    assert_ok(&env.run(
        root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "init",
            path,
        ],
    ));
    let output = env.run(
        root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--json",
            "commit",
            "commit",
            path,
            "--range",
            "0",
            "1",
            "--reason",
            "track",
        ],
    );
    assert_ok(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    value["data"]["commit"].as_str().unwrap().to_string()
}

fn register(env: &Env, cwd: &Path, alias: &str, root: &Path, meta: &Path) -> Output {
    let observed = env.run(
        cwd,
        &[
            "--root",
            root.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--json",
            "verify",
        ],
    );
    let value: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    let evidence = value["data"]["expected"].to_string();
    env.run(
        cwd,
        &[
            "--expected",
            evidence.as_str(),
            "project",
            "register",
            alias,
            root.to_str().unwrap(),
            meta.to_str().unwrap(),
        ],
    )
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        let dst = target.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(src, dst).unwrap();
        }
    }
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(base: &Path, dir: &Path, rows: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, rows);
            } else {
                rows.push((
                    path.strip_prefix(base).unwrap().to_path_buf(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut rows = Vec::new();
    if root.exists() {
        walk(root, root, &mut rows);
    }
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    rows
}

fn counted_command(root: &Path) {
    let script = root.join("source.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nn=0\n[ ! -f count ] || n=$(cat count)\necho $((n+1)) > count\nprintf '\\200\\r\\n'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn command_init(env: &Env, project: &Path, meta: &Path) -> Output {
    env.run(
        project,
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
            "[\"source.sh\"]",
            "commit",
            "init",
            "virtual.txt",
        ],
    )
}

#[test]
fn non_init_command_does_not_create_missing_metadata() {
    let env = Env::new();
    let project = env.root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();

    let output = env.run(&project, &["--root", project.to_str().unwrap(), "verify"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!project.join(".omd").exists());
}

#[test]
fn escaping_path_is_rejected_before_metadata_creation() {
    let env = Env::new();
    let project = env.root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(env.root.path().join("outside.md"), "x").unwrap();

    let output = env.run(
        &project,
        &["--root", project.to_str().unwrap(), "init", "../outside.md"],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(!project.join(".omd").exists());
}

#[test]
fn configuration_only_default_metadata_initializes_with_preserved_encoding() {
    let env = Env::new();
    let project = env.root.path().join("configured");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(project.join("doc.txt"), [0x80, b'\r', b'\n']).unwrap();
    let config = b"format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n";
    std::fs::write(meta.join("omd.toml"), config).unwrap();
    assert!(omd::records::store::Store::open(&meta).is_err());
    assert_eq!(std::fs::read(meta.join("omd.toml")).unwrap(), config);
    let before_init = env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "verify",
        ],
    );
    assert_eq!(before_init.status.code(), Some(2));
    assert_eq!(
        snapshot(&meta),
        vec![(PathBuf::from("omd.toml"), config.to_vec())]
    );
    assert!(!env.config.join("projects.toml").exists());

    let output = env.run(&project, &["--json", "init", "doc.txt"]);

    assert_ok(&output);
    assert_eq!(std::fs::read(meta.join("omd.toml")).unwrap(), config);
    let verify = env.run(
        &project,
        &[
            "--json",
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "verify",
        ],
    );
    assert_ok(&verify);
    let expected =
        serde_json::from_slice::<serde_json::Value>(&verify.stdout).unwrap()["data"]["expected"]
            .to_string();
    let range = env.run(
        &project,
        &[
            "--json",
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "--expected",
            &expected,
            "commit",
            "commit",
            "doc.txt",
            "--range",
            "0",
            "1",
            "--mode",
            "text",
            "--reason",
            "configured text",
        ],
    );
    assert_ok(&range);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let commit = value["data"]["commit"].as_str().unwrap();
    let store = omd::records::store::Store::open_existing(&meta).unwrap();
    let commit = store.read_commit(commit).unwrap();
    let version = store.read_version(&commit.content_ref).unwrap();
    assert_eq!(version.encoding.as_deref(), Some("windows-1252"));
    assert_eq!(version.len, 3);
    assert_eq!(
        version.sha256,
        omd::records::version::SourceVersion::content_sha256(&[0x80, b'\r', b'\n'])
    );
    assert_eq!(
        std::fs::read(meta.join(version.content_file.unwrap())).unwrap(),
        [0x80, b'\r', b'\n']
    );
}

#[test]
fn configuration_only_external_metadata_is_not_a_bootstrap_target() {
    let env = Env::new();
    let project = env.root.path().join("external-configured");
    let meta = env.root.path().join("metadata");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(project.join("doc.txt"), [0x80]).unwrap();
    let config = b"format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n";
    std::fs::write(meta.join("omd.toml"), config).unwrap();

    let output = env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "init",
            "doc.txt",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        snapshot(&meta),
        vec![(PathBuf::from("omd.toml"), config.to_vec())]
    );
    assert!(!env.config.join("projects.toml").exists());
}

#[test]
fn configuration_only_command_init_collects_once() {
    let env = Env::new();
    let project = env.root.path().join("configured-command-once");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&meta).unwrap();
    let config = b"format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n";
    std::fs::write(meta.join("omd.toml"), config).unwrap();
    counted_command(&project);

    let output = command_init(&env, &project, &meta);

    assert_ok(&output);
    assert_eq!(
        std::fs::read_to_string(project.join("count")).unwrap(),
        "1\n"
    );
    assert_eq!(std::fs::read(meta.join("omd.toml")).unwrap(), config);
}

#[test]
fn configuration_only_bootstrap_rejects_other_content_before_command_execution() {
    let env = Env::new();
    let project = env.root.path().join("configured-command");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(
        meta.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n",
    )
    .unwrap();
    std::fs::write(meta.join("unexpected"), b"keep").unwrap();
    counted_command(&project);
    let before = snapshot(&meta);

    let output = command_init(&env, &project, &meta);

    assert_eq!(output.status.code(), Some(2));
    assert!(!project.join("count").exists());
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn invalid_configuration_only_bootstrap_rejects_before_command_execution() {
    let env = Env::new();
    let project = env.root.path().join("invalid-configured-command");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&meta).unwrap();
    let config = b"format = \"omd.encoding/1\"\ndefault_encoding = \"not-an-encoding\"\n";
    std::fs::write(meta.join("omd.toml"), config).unwrap();
    counted_command(&project);

    let output = command_init(&env, &project, &meta);

    assert!(!output.status.success());
    assert!(!project.join("count").exists());
    assert_eq!(
        snapshot(&meta),
        vec![(PathBuf::from("omd.toml"), config.to_vec())]
    );
    assert!(!env.config.join("projects.toml").exists());
}

#[test]
fn configuration_only_bootstrap_rejects_previously_authorized_target() {
    let env = Env::new();
    let project = env.root.path().join("bound");
    let meta = project.join(".omd");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("doc.txt"), "old").unwrap();
    assert_ok(&env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "init",
            "doc.txt",
        ],
    ));
    let config = b"format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n";
    for entry in std::fs::read_dir(&meta).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            std::fs::remove_dir_all(path).unwrap();
        } else {
            std::fs::remove_file(path).unwrap();
        }
    }
    std::fs::write(meta.join("omd.toml"), config).unwrap();
    counted_command(&project);
    let authority_before = std::fs::read(env.config.join("projects.toml")).unwrap();
    let metadata_before = snapshot(&meta);

    let output = command_init(&env, &project, &meta);

    assert_eq!(output.status.code(), Some(2));
    assert!(!project.join("count").exists());
    assert_eq!(snapshot(&meta), metadata_before);
    assert_eq!(
        std::fs::read(env.config.join("projects.toml")).unwrap(),
        authority_before
    );
}

#[test]
fn unsupported_manifest_is_refused_without_writes() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = project.join(".omd");
    init_range(&env, &project, &meta, "x\n");
    let manifest = meta.join("manifest.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("omd.project/2", "omd.project/1")).unwrap();
    let before = snapshot(&meta);

    let output = env.run(&project, &["verify"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn previous_state_generation_is_refused_without_writes() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = project.join(".omd");
    init_range(&env, &project, &meta, "x\n");
    let state = meta.join("state.toml");
    let text = std::fs::read_to_string(&state).unwrap();
    std::fs::write(&state, text.replace("omd.state/9", "omd.state/7")).unwrap();
    let before = snapshot(&meta);

    let output = env.run(&project, &["verify"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(snapshot(&meta), before);
}

#[test]
fn wrong_identity_registration_changes_neither_authority() {
    let env = Env::new();
    let root_a = env.root.path().join("a");
    let meta_a = env.root.path().join("meta-a");
    init_range(&env, &root_a, &meta_a, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &root_a, &meta_a));

    let root_b = env.root.path().join("b");
    let meta_b = env.root.path().join("meta-b");
    init_range(&env, &root_b, &meta_b, "x\n");
    let config_before = std::fs::read(env.config.join("projects.toml")).unwrap();
    let manifest_before = std::fs::read(meta_b.join("manifest.toml")).unwrap();

    let output = register(&env, env.root.path(), "app", &root_b, &meta_b);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        std::fs::read(env.config.join("projects.toml")).unwrap(),
        config_before
    );
    assert_eq!(
        std::fs::read(meta_b.join("manifest.toml")).unwrap(),
        manifest_before
    );
}

#[test]
fn mapping_registration_does_not_activate_copied_store() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = env.root.path().join("metadata");
    init_range(&env, &project, &meta, "x\n");
    let state = meta.join("state.toml");
    let text = std::fs::read_to_string(&state).unwrap();
    std::fs::write(
        &state,
        text.replace("activated = true", "activated = false"),
    )
    .unwrap();

    let output = register(&env, env.root.path(), "copy", &project, &meta);

    assert_ok(&output);
    let after = std::fs::read_to_string(state).unwrap();
    assert!(after.contains("activated = false"));
}

#[test]
fn unsupported_projects_generation_is_not_treated_as_empty() {
    let env = Env::new();
    let file = env.config.join("projects.toml");
    std::fs::write(&file, "format = \"omd.projects/2\"\n").unwrap();
    let before = std::fs::read(&file).unwrap();

    let output = env.run(env.root.path(), &["project", "list"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(file).unwrap(), before);
}

#[test]
fn corrupt_projects_file_is_not_an_empty_success() {
    let env = Env::new();
    let file = env.config.join("projects.toml");
    std::fs::write(&file, "not = [valid").unwrap();
    let before = std::fs::read(&file).unwrap();

    let output = env.run(env.root.path(), &["project", "list"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(file).unwrap(), before);
}

#[test]
fn invalid_registration_leaves_local_config_unchanged() {
    let env = Env::new();
    let file = env.config.join("projects.toml");
    let before = std::fs::read(&file).ok();
    let missing_root = env.root.path().join("missing-root");
    let missing_meta = env.root.path().join("missing-meta");

    let output = register(&env, env.root.path(), "app", &missing_root, &missing_meta);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(&file).ok(), before);
}

#[test]
fn equal_depth_registered_roots_are_ambiguous() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta_a = env.root.path().join("meta-a");
    let meta_b = env.root.path().join("meta-b");
    std::fs::create_dir_all(&project).unwrap();
    let a = omd::records::store::Store::open(&meta_a)
        .unwrap()
        .identity();
    let b = omd::records::store::Store::open(&meta_b)
        .unwrap()
        .identity();
    let config = format!(
        "format = \"omd.projects/4\"\n\n[[project]]\nalias = \"a\"\nproject_id = \"{}\"\nstore_id = \"{}\"\nproject_root = \"{}\"\nmetadata_root = \"{}\"\nrevision = 1\n\n[[project]]\nalias = \"b\"\nproject_id = \"{}\"\nstore_id = \"{}\"\nproject_root = \"{}\"\nmetadata_root = \"{}\"\nrevision = 2\n",
        a.project_id,
        a.store_id,
        project.display(),
        meta_a.display(),
        b.project_id,
        b.store_id,
        project.display(),
        meta_b.display(),
    );
    std::fs::write(env.config.join("projects.toml"), config).unwrap();

    let output = env.run(&project, &["verify"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ambiguous"));
}

#[test]
fn subdirectory_verify_reads_selected_project_root() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = project.join(".omd");
    init_range(&env, &project, &meta, "x\n");
    let sub = project.join("sub/deep");
    std::fs::create_dir_all(&sub).unwrap();

    let output = env.run(&sub, &["verify"]);

    assert_ok(&output);
}

#[test]
fn explicit_external_metadata_keeps_project_root_for_observation() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = env.root.path().join("external-meta");
    init_range(&env, &project, &meta, "x\n");
    let sub = project.join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    let output = env.run(
        &sub,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            meta.to_str().unwrap(),
            "verify",
        ],
    );

    assert_ok(&output);
}

#[test]
fn explicit_metadata_overrides_root_mapping_but_not_explicit_alias_identity() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let mapped = project.join(".omd");
    init_range(&env, &project, &mapped, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &project, &mapped));
    let explicit = env.root.path().join("explicit-meta");
    omd::records::store::Store::open(&explicit).unwrap();

    let override_output = env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            explicit.to_str().unwrap(),
            "list",
        ],
    );
    assert_ok(&override_output);

    let alias_output = env.run(
        &project,
        &[
            "--project",
            "app",
            "--meta",
            explicit.to_str().unwrap(),
            "list",
        ],
    );
    assert_eq!(alias_output.status.code(), Some(2));
}

#[test]
fn explicit_new_metadata_can_initialize_despite_root_mapping() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let mapped = project.join(".omd");
    init_range(&env, &project, &mapped, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &project, &mapped));
    let explicit = env.root.path().join("explicit-meta");
    std::fs::write(project.join("b.md"), "y\n").unwrap();

    let output = env.run(
        &project,
        &[
            "--root",
            project.to_str().unwrap(),
            "--meta",
            explicit.to_str().unwrap(),
            "init",
            "b.md",
        ],
    );

    assert_ok(&output);
    assert!(explicit.join("manifest.toml").exists());
}

#[test]
fn mapped_missing_metadata_cannot_be_reinitialized() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = env.root.path().join("metadata");
    init_range(&env, &project, &meta, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &project, &meta));

    let saved_meta = env.root.path().join("saved-metadata");
    std::fs::rename(&meta, &saved_meta).unwrap();
    std::fs::write(project.join("b.md"), "y\n").unwrap();
    let saved_before = snapshot(&saved_meta);
    let config_before = std::fs::read(env.config.join("projects.toml")).unwrap();

    for args in [
        vec!["--project", "app", "init", "b.md"],
        vec!["init", "b.md"],
    ] {
        let output = env.run(&project, &args);
        assert_eq!(output.status.code(), Some(2));
        assert!(!meta.exists());
        assert_eq!(snapshot(&saved_meta), saved_before);
        assert_eq!(
            std::fs::read(env.config.join("projects.toml")).unwrap(),
            config_before
        );
    }
}

#[test]
fn explicit_nested_root_uses_its_own_metadata_for_reads_and_writes() {
    let env = Env::new();
    let parent = env.root.path().join("parent");
    let parent_meta = parent.join(".omd");
    let parent_commit = init_named_range(&env, &parent, &parent_meta, "a.md", "p\n");
    assert_ok(&register(
        &env,
        env.root.path(),
        "parent",
        &parent,
        &parent_meta,
    ));

    let child = parent.join("child");
    let child_meta = child.join(".omd");
    let child_commit = init_named_range(&env, &child, &child_meta, "b.md", "c\n");

    let list = env.run(
        env.root.path(),
        &["--root", child.to_str().unwrap(), "--json", "list"],
    );
    assert_ok(&list);
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let tips = value["data"]["tips"].to_string();
    assert!(tips.contains(&child_commit), "child tips missing: {tips}");
    assert!(!tips.contains(&parent_commit), "parent tips leaked: {tips}");

    std::fs::write(child.join("c.md"), "child-only\n").unwrap();
    let parent_before = snapshot(&parent_meta);
    let child_before = snapshot(&child_meta);
    let mutation = env.run(
        env.root.path(),
        &["--root", child.to_str().unwrap(), "init", "c.md"],
    );
    assert_ok(&mutation);
    assert_eq!(snapshot(&parent_meta), parent_before);
    assert_ne!(snapshot(&child_meta), child_before);
}

#[test]
fn duplicate_exact_mapping_is_not_ambiguous() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let meta = project.join(".omd");
    init_range(&env, &project, &meta, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &project, &meta));

    let config = env.config.join("projects.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    let record = text.split("[[project]]").nth(1).unwrap();
    std::fs::write(&config, format!("{text}\n[[project]]{record}")).unwrap();

    assert_ok(&env.run(env.root.path(), &["--project", "app", "list"]));
    assert_ok(&env.run(env.root.path(), &["--project", "app", "verify"]));
}

#[test]
fn mapped_missing_metadata_never_falls_back_to_nearby_dot_omd() {
    let env = Env::new();
    let project = env.root.path().join("project");
    let valid_meta = project.join(".omd");
    init_range(&env, &project, &valid_meta, "x\n");
    assert_ok(&register(
        &env,
        env.root.path(),
        "app",
        &project,
        &valid_meta,
    ));

    let config = env.config.join("projects.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        text.replace(
            valid_meta.to_str().unwrap(),
            env.root.path().join("gone").to_str().unwrap(),
        ),
    )
    .unwrap();

    let output = env.run(&project, &["--project", "app", "verify"]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn two_machine_configs_read_their_own_roots_without_recomputing_ids() {
    let env_a = Env::new();
    let root_a = env_a.root.path().join("checkout");
    let meta_a = env_a.root.path().join("metadata");
    let commit = init_range(&env_a, &root_a, &meta_a, "x\n");
    assert_ok(&register(
        &env_a,
        env_a.root.path(),
        "app",
        &root_a,
        &meta_a,
    ));

    let env_b = Env::new();
    let root_b = env_b.root.path().join("checkout");
    let meta_b = env_b.root.path().join("metadata");
    std::fs::create_dir_all(&root_b).unwrap();
    std::fs::write(root_b.join("a.md"), "y\n").unwrap();
    copy_tree(&meta_a, &meta_b);
    let identity_a = omd::records::store::Store::identity_at(&meta_a).unwrap();
    let identity_b = omd::records::store::Store::identity_at(&meta_b).unwrap();
    assert_eq!(identity_a.project_id, identity_b.project_id);
    assert_eq!(identity_a.store_id, identity_b.store_id);
    assert_ok(&register(
        &env_b,
        env_b.root.path(),
        "app",
        &root_b,
        &meta_b,
    ));

    let a = env_a.run(env_a.root.path(), &["--project", "app", "verify"]);
    let b = env_b.run(env_b.root.path(), &["--project", "app", "verify"]);
    assert_ok(&a);
    assert_eq!(b.status.code(), Some(1));

    let log = env_b.run(
        env_b.root.path(),
        &["--project", "app", "--json", "log", &commit],
    );
    assert_ok(&log);
    let value: serde_json::Value = serde_json::from_slice(&log.stdout).unwrap();
    assert_eq!(value["data"]["chain"][0], commit);
}

#[test]
fn store_and_project_root_are_compatible_context_selectors() {
    let env = Env::new();
    let root = env.root.path().join("project");
    let meta = env.root.path().join("metadata");
    init_range(&env, &root, &meta, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &root, &meta));

    let store_with_root = env.run(
        env.root.path(),
        &["--store", "app", "--project", "root", "verify"],
    );
    assert_ok(&store_with_root);
    let project_with_root = env.run(
        env.root.path(),
        &["--store", "root", "--project", "app", "verify"],
    );
    assert_ok(&project_with_root);
    let conflict = env.run(
        env.root.path(),
        &["--store", "app", "--project", "other", "verify"],
    );
    assert_eq!(conflict.status.code(), Some(2));
}

#[test]
fn relocation_updates_only_local_placement() {
    let env = Env::new();
    let old_root = env.root.path().join("old");
    let meta = env.root.path().join("metadata");
    let commit = init_range(&env, &old_root, &meta, "x\n");
    assert_ok(&register(&env, env.root.path(), "app", &old_root, &meta));

    let new_root = env.root.path().join("new");
    std::fs::rename(&old_root, &new_root).unwrap();
    assert_ok(&register(&env, env.root.path(), "app", &new_root, &meta));

    let output = env.run(
        env.root.path(),
        &["--project", "app", "--json", "log", &commit],
    );
    assert_ok(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["chain"][0], commit);
}
