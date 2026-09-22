use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn omd() -> PathBuf {
    option_env!("CARGO_BIN_EXE_omd")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            path.pop();
            path.push("omd");
            path
        })
}

struct Env {
    tmp: tempfile::TempDir,
    root: PathBuf,
    meta: PathBuf,
    config: PathBuf,
    cache: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        Self {
            meta: root.join(".omd"),
            config: tmp.path().join("config"),
            cache: tmp.path().join("cache"),
            tmp,
            root,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(omd())
            .args([
                "--json",
                "--root",
                self.root.to_str().unwrap(),
                "--meta",
                self.meta.to_str().unwrap(),
            ])
            .args(args)
            .current_dir(&self.root)
            .env("HOME", self.tmp.path().join("home"))
            .env("OMD_CONFIG_PATH", &self.config)
            .env("OMD_CACHE_PATH", &self.cache)
            .output()
            .unwrap()
    }

    fn expected(&self, extra: &[&str]) -> String {
        let mut args = extra.to_vec();
        args.push("verify");
        let output = self.run(&args);
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        value["data"]["expected"].to_string()
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn register_project(&self, alias: &str, root: &Path, tracked: &str) {
        let meta = root.join(".omd");
        let run = |args: &[&str]| {
            Command::new(omd())
                .arg("--json")
                .arg("--root")
                .arg(root)
                .arg("--meta")
                .arg(&meta)
                .args(args)
                .current_dir(root)
                .env("HOME", self.tmp.path().join("home"))
                .env("OMD_CONFIG_PATH", &self.config)
                .env("OMD_CACHE_PATH", &self.cache)
                .output()
                .unwrap()
        };
        assert_ok(&run(&["init", tracked]));
        let observed = assert_ok(&run(&["verify"]));
        let expected = observed["data"]["expected"].to_string();
        assert_ok(&run(&[
            "--expected",
            &expected,
            "project",
            "register",
            alias,
            root.to_str().unwrap(),
            meta.to_str().unwrap(),
        ]));
    }
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

fn version_for(env: &Env, path: &str) -> omd::records::version::SourceVersion {
    let state: omd::records::store::State =
        toml::from_str(&std::fs::read_to_string(env.meta.join("state.toml")).unwrap()).unwrap();
    let node = state
        .locations
        .iter()
        .find_map(|(node, location)| (location == path).then_some(node))
        .unwrap();
    let commit: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(env.meta.join(format!("commits/{}.toml", state.tips[node])))
            .unwrap(),
    )
    .unwrap();
    toml::from_str(
        &std::fs::read_to_string(
            env.meta
                .join(format!("versions/{}.toml", commit.content_ref)),
        )
        .unwrap(),
    )
    .unwrap()
}

fn assert_ok(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn command_fields_execute_once_and_existing_commit_reuses_observation() {
    let env = Env::new();
    env.write(
        "source.sh",
        b"#!/bin/sh\nn=0; [ ! -f count ] || n=$(cat count); echo $((n+1)) > count\nprintf '%s\\n' \"$PWD\" > observed-cwd\nprintf '<%s>\\n' \"$@\" > observed-args\nprintf payload\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            env.root.join("source.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    assert_ok(&env.run(&[
        "--source-type",
        "command",
        "--executable",
        "/bin/sh",
        "--args-json",
        "[\"source.sh\",\"\",\"a b\",\"::\"]",
        "init",
        "virtual::name",
    ]));
    assert_eq!(
        std::fs::read_to_string(env.root.join("count")).unwrap(),
        "1\n"
    );
    assert_eq!(
        std::fs::read_to_string(env.root.join("observed-cwd"))
            .unwrap()
            .trim(),
        env.root.to_str().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(env.root.join("observed-args")).unwrap(),
        "<>\n<a b>\n<::>\n"
    );

    let expected = env.expected(&["--run-command=true"]);
    assert_eq!(
        std::fs::read_to_string(env.root.join("count")).unwrap(),
        "2\n"
    );
    assert_ok(&env.run(&[
        "--expected",
        &expected,
        "commit",
        "commit",
        "virtual::name",
        "--reason",
        "record observed output",
    ]));
    assert_eq!(
        std::fs::read_to_string(env.root.join("count")).unwrap(),
        "2\n",
        "commit must consume returned observation without rerunning command"
    );

    let state: omd::records::store::State =
        toml::from_str(&std::fs::read_to_string(env.meta.join("state.toml")).unwrap()).unwrap();
    let tip = state.tips.values().next().unwrap().clone();
    assert_ok(&env.run(&["log", &tip]));
    assert_ok(&env.run(&["tree"]));
    assert_ok(&env.run(&["reindex"]));
    assert_eq!(
        std::fs::read_to_string(env.root.join("count")).unwrap(),
        "2\n"
    );
}

#[test]
fn incompatible_fields_reject_before_command_or_store_creation() {
    let env = Env::new();
    env.write("ran.sh", b"#!/bin/sh\ntouch ran\nprintf payload\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            env.root.join("ran.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let output = env.run(&[
        "--source-type",
        "file",
        "--executable",
        "./ran.sh",
        "init",
        "target",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!env.root.join("ran").exists());
    assert!(
        !env.meta.exists(),
        "invalid source fields publish no authority"
    );
}

#[test]
fn scheme_looking_file_source_path_is_literal() {
    let env = Env::new();
    env.write("command::not-a-command", b"literal bytes\n");
    let output = env.run(&[
        "--source-type",
        "file",
        "--source-path",
        "command::not-a-command",
        "init",
        "tracked-name",
    ]);
    assert_ok(&output);
    let versions = std::fs::read_dir(env.meta.join("versions")).unwrap();
    let text = versions
        .filter_map(Result::ok)
        .find_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .unwrap();
    assert!(text.contains("command::not-a-command"), "{text}");
    assert!(text.contains("type = \"file\""), "{text}");
}

#[test]
fn file_alias_init_observes_target_and_preserves_relative_recovery() {
    let env = Env::new();
    let upstream = env.tmp.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    std::fs::write(upstream.join("doc.md"), "abcdefghij").unwrap();
    env.register_project("upstream", &upstream, "doc.md");
    env.write("live.md", b"abcdefghij");

    assert_ok(&env.run(&[
        "--source-type",
        "file",
        "--source-project",
        "upstream",
        "--source-path",
        upstream.join("doc.md").to_str().unwrap(),
        "init",
        "live.md",
    ]));
    let version_file = std::fs::read_dir(env.meta.join("versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let version: omd::records::version::SourceVersion =
        toml::from_str(&std::fs::read_to_string(version_file).unwrap()).unwrap();
    assert_eq!(
        version.acquisition,
        omd::sources::SourceDescriptor::File {
            project: "root".into(),
            path: "live.md".into(),
        }
    );
    assert_eq!(
        version.recovery,
        omd::sources::SourceDescriptor::File {
            project: "upstream".into(),
            path: "doc.md".into(),
        }
    );
    assert!(assert_ok(&env.run(&["verify"]))["data"]["ok"] == true);
    std::fs::remove_file(env.root.join("live.md")).unwrap();
    let missing = env.run(&["verify"]);
    assert_eq!(missing.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert!(
        value["data"]["missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry
                    .as_str()
                    .is_some_and(|entry| entry.contains("live.md"))
            })
    );
}

#[test]
fn wrong_source_mapping_identity_rejects_before_owner_publication() {
    let env = Env::new();
    let source = env.tmp.path().join("source");
    let other = env.tmp.path().join("other");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(source.join("doc.md"), "same").unwrap();
    std::fs::write(other.join("doc.md"), "same").unwrap();
    env.register_project("upstream", &source, "doc.md");
    env.register_project("unrelated", &other, "doc.md");
    std::fs::remove_dir_all(source.join(".omd")).unwrap();
    copy_dir(&other.join(".omd"), &source.join(".omd"));
    env.write("owned.md", b"same");
    assert_ok(&env.run(&["init", "owned.md"]));
    let state_before = std::fs::read(env.meta.join("state.toml")).unwrap();
    let state: omd::records::store::State =
        toml::from_str(&String::from_utf8(state_before.clone()).unwrap()).unwrap();
    let tip = state.tips.values().next().unwrap();
    let expected = env.expected(&[]);
    let output = env.run(&[
        "--expected",
        &expected,
        "replace",
        tip,
        "--source-type",
        "file",
        "--source-project",
        "upstream",
        "--source-path",
        "doc.md",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        std::fs::read(env.meta.join("state.toml")).unwrap(),
        state_before
    );
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn exact_git_init_records_file_observation_and_git_recovery() {
    let env = Env::new();
    env.write("tracked.md", b"historical bytes\n");
    let repo = env.tmp.path().join("history");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("old.md"), "historical bytes\n").unwrap();
    git(&repo, &["add", "old.md"]);
    git(
        &repo,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "old"],
    );
    let commit = git(&repo, &["rev-parse", "HEAD"]);
    env.register_project("history", &repo, "old.md");

    assert_ok(&env.run(&[
        "--source-type",
        "git",
        "--source-project",
        "history",
        "--git-commit",
        &commit,
        "--git-path",
        "old.md",
        "init",
        "tracked.md",
    ]));
    let version_file = std::fs::read_dir(env.meta.join("versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let version: omd::records::version::SourceVersion =
        toml::from_str(&std::fs::read_to_string(version_file).unwrap()).unwrap();
    assert_eq!(
        version.acquisition,
        omd::sources::SourceDescriptor::File {
            project: "root".into(),
            path: "tracked.md".into(),
        }
    );
    assert_eq!(
        version.recovery,
        omd::sources::SourceDescriptor::Git {
            project: "history".into(),
            commit,
            path: "old.md".into(),
        }
    );
}

#[test]
fn missing_git_object_init_publishes_nothing() {
    let env = Env::new();
    env.write("tracked.md", b"current bytes\n");
    let repo = env.tmp.path().join("history");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("seed.md"), "seed").unwrap();
    env.register_project("history", &repo, "seed.md");
    let output = env.run(&[
        "--source-type",
        "git",
        "--source-project",
        "history",
        "--git-commit",
        &"0".repeat(40),
        "--git-path",
        "missing.md",
        "init",
        "tracked.md",
    ]);
    assert_ne!(output.status.code(), Some(0));
    assert!(!env.meta.exists());
}

#[test]
fn configured_encoding_precedence_and_recorded_view_are_runtime_effective() {
    let env = Env::new();
    std::fs::create_dir_all(&env.config).unwrap();
    std::fs::write(
        env.config.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n",
    )
    .unwrap();
    env.write("user.md", [0x80, b'\r', b'\n']);
    assert_ok(&env.run(&["init", "user.md"]));
    let user = version_for(&env, "user.md");
    assert_eq!(user.encoding.as_deref(), Some("windows-1252"));
    assert_eq!(
        std::fs::read(env.meta.join(user.content_file.as_ref().unwrap())).unwrap(),
        [0x80, b'\r', b'\n']
    );

    std::fs::write(
        env.config.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"utf-8\"\n",
    )
    .unwrap();
    std::fs::write(
        env.meta.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"windows-1252\"\n",
    )
    .unwrap();
    env.write("project.md", [0x80]);
    assert_ok(&env.run(&["init", "project.md"]));
    assert_eq!(
        version_for(&env, "project.md").encoding.as_deref(),
        Some("windows-1252")
    );

    std::fs::write(
        env.meta.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"utf-8\"\n\n[file.\"exact.md\"]\nencoding = \"windows-1252\"\n",
    )
    .unwrap();
    env.write("exact.md", [0x80]);
    assert_ok(&env.run(&["init", "exact.md"]));
    assert_eq!(
        version_for(&env, "exact.md").encoding.as_deref(),
        Some("windows-1252")
    );

    std::fs::write(
        env.meta.join("omd.toml"),
        "format = \"omd.encoding/1\"\ndefault_encoding = \"utf-8\"\n",
    )
    .unwrap();
    let verify = env.run(&["verify"]);
    assert_ok(&verify);
    assert_eq!(
        version_for(&env, "exact.md").encoding.as_deref(),
        Some("windows-1252"),
        "later defaults never rewrite the recorded view"
    );
}

#[test]
fn omitted_encoding_uses_utf8_and_byte_mode_never_decodes() {
    let env = Env::new();
    env.write("utf8.md", "汉字\n".as_bytes());
    assert_ok(&env.run(&["init", "utf8.md"]));
    assert_eq!(
        version_for(&env, "utf8.md").encoding.as_deref(),
        Some("utf-8")
    );

    env.write("raw.bin", [0xff, 0x00]);
    let raw = omd::sources::collect(
        &omd::sources::SourceDescriptor::File {
            project: "root".into(),
            path: "raw.bin".into(),
        },
        &env.root,
        &env.root,
        false,
        Some("not-an-encoding"),
    )
    .unwrap();
    assert_eq!(raw.bytes, [0xff, 0x00]);
    assert_eq!(raw.encoding, None);
}

#[test]
fn explicit_encoding_on_existing_commit_records_selected_view() {
    let env = Env::new();
    env.write("text.md", "éA".as_bytes());
    assert_ok(&env.run(&["--encoding", "utf-8", "init", "text.md"]));
    let expected = env.expected(&[]);
    let value = assert_ok(&env.run(&[
        "--encoding",
        "windows-1252",
        "--expected",
        &expected,
        "commit",
        "commit",
        "text.md",
        "--reason",
        "explicit view",
    ]));
    let commit = value["data"]["commit"].as_str().unwrap();
    let commit: omd::records::commit::Commit = toml::from_str(
        &std::fs::read_to_string(env.meta.join(format!("commits/{commit}.toml"))).unwrap(),
    )
    .unwrap();
    let version: omd::records::version::SourceVersion = toml::from_str(
        &std::fs::read_to_string(
            env.meta
                .join(format!("versions/{}.toml", commit.content_ref)),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(version.encoding.as_deref(), Some("windows-1252"));
}

#[test]
fn unknown_encoding_existing_commit_rejects_without_publication() {
    let env = Env::new();
    env.write("text.md", "éA".as_bytes());
    assert_ok(&env.run(&["--encoding", "utf-8", "init", "text.md"]));
    let before = std::fs::read(&env.meta.join("state.toml")).unwrap();
    let output = env.run(&[
        "--encoding",
        "not-an-encoding",
        "commit",
        "commit",
        "text.md",
        "--reason",
        "invalid view",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(env.meta.join("state.toml")).unwrap(), before);
}

#[test]
fn legacy_source_ref_is_rejected_without_execution() {
    let env = Env::new();
    env.write("ran.sh", b"#!/bin/sh\ntouch ran\nprintf payload\n");
    let output = env.run(&["--source-ref", "command::./ran.sh::[]", "init", "legacy"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!env.root.join("ran").exists());
    assert!(!env.meta.exists());
}
