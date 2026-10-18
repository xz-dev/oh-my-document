mod common;

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn omd() -> std::path::PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_omd") {
        return path.into();
    }
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("omd");
    path
}

struct Fixture {
    root: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "omd-query-coverage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn run_raw(&self, args: &[&str]) -> (i32, serde_json::Value, String) {
        let output = Command::new(omd())
            .arg("--json")
            .arg("--root")
            .arg(&self.root)
            .arg("--meta")
            .arg(self.root.join(".omd"))
            .args(args)
            .current_dir(&self.root)
            .env("HOME", self.root.join("home"))
            .env("OMD_CONFIG_PATH", self.root.join("config"))
            .env("OMD_CACHE_PATH", self.root.join("cache"))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
            panic!(
                "invalid JSON ({error}): stdout={stdout} stderr={}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (
            output.status.code().unwrap_or(-1),
            value,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    fn run(&self, args: &[&str]) -> (i32, serde_json::Value, String) {
        let meta = self.root.join(".omd");
        let mut command = Command::new(omd());
        command
            .arg("--json")
            .arg("--meta")
            .arg(&meta)
            .args(common::with_expected(
                &omd(),
                &self.root,
                args,
                Some(&meta),
                Some(&self.root.join("home")),
                Some(&self.root.join("config")),
                Some(&self.root.join("cache")),
            ))
            .current_dir(&self.root)
            .env("HOME", self.root.join("home"))
            .env("OMD_CONFIG_PATH", self.root.join("config"))
            .env("OMD_CACHE_PATH", self.root.join("cache"));
        let output = command.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
            panic!(
                "invalid JSON ({error}): stdout={stdout} stderr={}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (
            output.status.code().unwrap_or(-1),
            value,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    fn ok(&self, args: &[&str]) -> serde_json::Value {
        let (code, value, stderr) = self.run(args);
        assert_eq!(code, 0, "{args:?}: {value} {stderr}");
        value
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn root_id(value: &serde_json::Value) -> String {
    value["data"]["object"]["node"]["root_commit_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn find_group<'a>(rule: &'a serde_json::Value, unit: &str) -> &'a serde_json::Value {
    rule["coverage"]["forward"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["unit"] == unit)
        .unwrap()
}

fn setup_unmarked_scope(fixture: &Fixture) {
    fixture.write("impl.rs", "X");
    fixture.write("spec/unmarked.txt", [0xe9]);
    fixture.ok(&["init", "impl.rs"]);
    fixture.ok(&["import", "spec"]);
    fixture.ok(&["commit", "tag", "spec", "--tag", "spec"]);
    fixture.ok(&[
        "commit",
        "scope_adjust",
        "spec",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
}

#[test]
fn file_import_is_independent_of_content_tracking() {
    let fixture = Fixture::new();
    fixture.write("seed.md", "seed");
    fixture.write("README.md", "abc");
    fixture.ok(&["init", "seed.md"]);
    let imported = fixture.ok(&["import", "README.md"]);
    let before = fixture.ok(&["check"]);
    assert_eq!(before["data"]["check"]["files"][0]["tracked"], false);
    assert_eq!(before["data"]["check"]["files"][0]["file"], "README.md");
    let initialized = fixture.ok(&["init", "README.md"]);
    assert_ne!(root_id(&imported), root_id(&initialized));
    let range = fixture.ok(&[
        "commit",
        "commit",
        "README.md",
        "--range",
        "0",
        "3",
        "--reason",
        "review",
    ]);
    fixture.ok(&["remove", "README.md"]);
    let removed = fixture.ok(&["check"]);
    assert!(
        removed["data"]["check"]["files"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fixture.ok(&["log", &root_id(&range)]);
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("README.md")).unwrap(),
        "abc"
    );
    fixture.ok(&["import", "README.md"]);
    let after = fixture.ok(&["check"]);
    assert_eq!(after["data"]["check"]["files"][0]["tracked"], true);
    fixture.write("other.md", "other");
    fixture.ok(&["init", "other.md"]);
    fixture.ok(&["import", "other.md"]);
    let (status, _, _) = fixture.run(&["commit", "tag", "other.md", "--tag", "spec"]);
    assert_eq!(status, 2, "same-path objects require an explicit tip");
    fixture.ok(&["rename", "other.md", "renamed.md"]);
    std::fs::rename(
        fixture.root.join("other.md"),
        fixture.root.join("renamed.md"),
    )
    .unwrap();
    fixture.ok(&["remove", "other.md"]);
    fixture.ok(&["import", "renamed.md"]);
    let final_report = fixture.ok(&["check"]);
    assert_eq!(
        final_report["data"]["check"]["files"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "{final_report}"
    );
}

#[test]
fn missing_imported_file_fails_without_silent_scope_loss() {
    let fixture = Fixture::new();
    fixture.write("seed.md", "seed");
    fixture.write("README.md", "abc");
    fixture.ok(&["init", "seed.md"]);
    fixture.ok(&["import", "README.md"]);
    std::fs::remove_file(fixture.root.join("README.md")).unwrap();
    let (code, value, _) = fixture.run_raw(&["check"]);
    assert_eq!(code, 1, "{value}");
    assert_eq!(value["data"]["check"]["incomplete"], true);
    assert!(
        !value["data"]["check"]["problems"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn revised_root_import_updates_listing_and_tag_coverage() {
    let fixture = Fixture::new();
    fixture.write("README.md", "abc");
    fixture.write("README.zh-CN.md", "de");
    fixture.write("unrelated.md", "excluded");
    fixture.write("docs/README.md", "excluded nested readme");
    fixture.write("target/artifact", "excluded");
    fixture.write(".git/artifact", "excluded");
    fixture.ok(&["init", "README.md"]);
    fixture.ok(&["import", ".", "--exclude", "*"]);
    fixture.ok(&["commit", "tag", ".", "--tag", "spec"]);
    fixture.ok(&[
        "import",
        ".",
        "--exclude",
        "*",
        "--include",
        "README.md",
        "--include",
        "README.zh-CN.md",
    ]);
    // Later tag/rule records must not hide the latest import's scope.
    fixture.ok(&["commit", "tag", ".", "--tag", "docs"]);
    fixture.ok(&[
        "commit",
        "scope_adjust",
        ".",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
    let report = fixture.ok(&["check"]);
    let files = report["data"]["check"]["files"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> = files
        .iter()
        .map(|file| file["file"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from(["README.md", "README.zh-CN.md"])
    );
    assert!(files.iter().all(|file| file["scope"] == ""));
    assert!(
        files
            .iter()
            .find(|file| file["file"] == "README.md")
            .unwrap()["tracked"]
            == true
    );
    let rule = &report["data"]["check"]["rules"][0];
    let text = find_group(rule, "text");
    assert_eq!(text["total"], "5");
    assert_eq!(text["covered"], "0");
    let covered_files = rule["coverage"]["forward"]["files"].as_array().unwrap();
    let paths: std::collections::BTreeSet<_> = covered_files
        .iter()
        .map(|file| file["project_relative_path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, names);
}

#[test]
fn json_parser_errors_use_schema_v2_envelope() {
    let fixture = Fixture::new();
    for args in [
        vec!["--not-an-option"],
        vec!["commit", "commit", "doc.md", "--range", "0"],
    ] {
        let (code, value, stderr) = fixture.run_raw(&args);
        assert_eq!(code, 2);
        assert_eq!(value["schema_version"], "2");
        assert_eq!(value["ok"], false);
        assert_eq!(value["diagnostics"][0]["kind"], "usage");
        assert!(stderr.is_empty());
    }
}

#[test]
fn dirty_verify_and_check_emit_contextual_reason() {
    let fixture = Fixture::new();
    fixture.write("doc.md", "abcdefghij");
    fixture.ok(&["init", "doc.md"]);
    let range = fixture.ok(&[
        "commit", "commit", "doc.md", "--range", "0", "4", "--reason", "seed",
    ]);
    let commit = range["data"]["commit"].as_str().unwrap();
    fixture.ok(&[
        "commit",
        "unclean",
        "doc.md",
        "--id",
        commit,
        "--reason",
        "explicit responsibility",
    ]);
    for command in ["verify", "check"] {
        let (code, value, _) = fixture.run(&[command]);
        assert_eq!(code, 1);
        let diagnostic = value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["kind"] == "dirty")
            .unwrap();
        assert_eq!(diagnostic["message"], "explicit responsibility");
        assert!(diagnostic["store"].is_string());
        assert!(diagnostic["node"].is_object());
        assert!(diagnostic["commit_id"].is_string());
    }
}

#[test]
fn unmarked_coverage_uses_runtime_encoding_precedence() {
    let configured = Fixture::new();
    setup_unmarked_scope(&configured);
    configured.write(
        ".omd/omd.toml",
        "format = \"omd.encoding/1\"\n[file.\"spec/unmarked.txt\"]\nencoding = \"windows-1252\"\n",
    );
    let checked = configured.ok(&["check"]);
    let row = &checked["data"]["check"]["rules"][0]["coverage"]["forward"]["files"][0];
    assert_eq!(row["status"], "complete");
    assert_eq!(row["total"], "1");

    let explicit = Fixture::new();
    setup_unmarked_scope(&explicit);
    let checked = explicit.ok(&["--encoding", "windows-1252", "check"]);
    let row = &checked["data"]["check"]["rules"][0]["coverage"]["forward"]["files"][0];
    assert_eq!(row["status"], "complete");
    assert_eq!(row["total"], "1");

    let invalid = Fixture::new();
    setup_unmarked_scope(&invalid);
    invalid.write("spec/unmarked.txt", "ABC");
    invalid.write(
        ".omd/omd.toml",
        "format = \"omd.encoding/1\"\n[file.\"spec/unmarked.txt\"]\nencoding = \"unknown-encoding-omd\"\n",
    );
    let checked = invalid.ok(&["check"]);
    let row = &checked["data"]["check"]["rules"][0]["coverage"]["forward"]["files"][0];
    assert_eq!(row["status"], "incomplete");
    assert!(row["percentage"].is_null());
}

#[test]
fn failed_text_view_keeps_byte_denominator() {
    let fixture = Fixture::new();
    fixture.write("source.txt", "ABCD");
    fixture.write("code.rs", "WXYZ");
    fixture.ok(&["init", "source.txt"]);
    fixture.ok(&["init", "code.rs"]);
    let text = fixture.ok(&[
        "commit",
        "commit",
        "source.txt",
        "--range",
        "0",
        "4",
        "--reason",
        "text",
    ]);
    let target = fixture.ok(&[
        "commit", "commit", "code.rs", "--range", "0", "4", "--reason", "target",
    ]);
    fixture.ok(&["commit", "tag", "source.txt", "--tag", "spec"]);
    fixture.ok(&["commit", "tag", "code.rs", "--tag", "code"]);
    fixture.ok(&[
        "commit",
        "scope_adjust",
        "source.txt",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);
    fixture.ok(&[
        "commit",
        "commit",
        "source.txt",
        "--range",
        "0",
        "2",
        "--mode",
        "byte",
        "--link-to",
        &format!("range:{}", root_id(&target)),
        "--reason",
        "byte",
    ]);
    fixture.write("source.txt", [0xff, b'B', b'C', b'D']);
    let (code, checked, _) = fixture.run(&["check"]);
    assert_eq!(code, 1);
    let files = checked["data"]["check"]["rules"][0]["coverage"]["forward"]["files"]
        .as_array()
        .unwrap();
    assert!(
        files
            .iter()
            .any(|row| row["unit"] == "text" && row["status"] == "incomplete")
    );
    assert!(
        files
            .iter()
            .any(|row| row["unit"] == "byte" && row["total"] == "4")
    );
    assert!(text["data"]["commit"].is_string());
}

#[test]
fn missing_source_marks_same_unit_aggregate_partial() {
    let fixture = Fixture::new();
    for (path, bytes) in [
        ("good.md", "ABCD"),
        ("missing.md", "EFGH"),
        ("code.rs", "IJKL"),
    ] {
        fixture.write(path, bytes);
        fixture.ok(&["init", path]);
    }
    let good = fixture.ok(&[
        "commit", "commit", "good.md", "--range", "0", "4", "--reason", "good",
    ]);
    let missing = fixture.ok(&[
        "commit",
        "commit",
        "missing.md",
        "--range",
        "0",
        "4",
        "--reason",
        "missing",
    ]);
    let target = fixture.ok(&[
        "commit", "commit", "code.rs", "--range", "0", "4", "--reason", "target",
    ]);
    for (path, tag) in [
        ("good.md", "spec"),
        ("missing.md", "spec"),
        ("code.rs", "code"),
    ] {
        fixture.ok(&["commit", "tag", path, "--tag", tag]);
    }
    let target = root_id(&target);
    for (path, source) in [
        ("good.md", root_id(&good)),
        ("missing.md", root_id(&missing)),
    ] {
        fixture.ok(&[
            "commit", "link", path, "--source", &source, "--target", &target, "--reason", "linked",
        ]);
    }
    fixture.ok(&[
        "commit",
        "scope_adjust",
        "good.md",
        "--rule",
        "spec->code",
        "--level",
        "fail",
    ]);
    assert_eq!(fixture.run(&["check"]).0, 0);
    std::fs::rename(
        fixture.root.join("missing.md"),
        fixture.root.join("offline-source"),
    )
    .unwrap();
    let (code, checked, _) = fixture.run(&["check"]);
    assert_eq!(code, 1);
    let forward = &checked["data"]["check"]["rules"][0]["coverage"]["forward"];
    assert_eq!(forward["status"], "incomplete");
    let text = forward["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["unit"] == "text")
        .unwrap();
    assert!(text["percentage"].is_null());
    assert_eq!(text["partial"], true);
}

#[test]
fn public_queries_keep_identity_version_location_and_link_fields_separate() {
    let fixture = Fixture::new();
    let old = "we@ird #1%:文.md";
    let new = "renamed @#%:文.md";
    fixture.write(old, "αβγδε");
    fixture.write("--lead.md", "lead");

    let init = fixture.ok(&["init", old]);
    assert_eq!(init["schema_version"], "2");
    assert_eq!(init["data"]["object"]["project_relative_path"], old);
    fixture.ok(&["init", "--", "--lead.md"]);

    let first = fixture.ok(&[
        "commit", "commit", old, "--range", "0", "2", "--reason", "first",
    ]);
    let second = fixture.ok(&[
        "commit", "commit", old, "--range", "0", "2", "--reason", "second",
    ]);
    let first_root = root_id(&first);
    let second_root = root_id(&second);
    assert_ne!(first_root, second_root);

    let link = fixture.ok(&[
        "commit",
        "link",
        old,
        "--source",
        &first_root,
        "--target",
        &second_root,
        "--reason",
        "independent",
    ]);
    let link_id = link["data"]["link"]["link_id"].as_str().unwrap();
    assert_eq!(
        link["data"]["link"]["creation_commit_id"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        link["data"]["link"]["source"]["object"]["root_commit_id"],
        first_root
    );
    assert_eq!(
        link["data"]["link"]["target"]["object"]["root_commit_id"],
        second_root
    );
    assert_eq!(
        link["data"]["link"]["source"]["object"]["scope"]["kind"],
        "local"
    );

    fixture.ok(&["rename", old, new]);
    std::fs::rename(fixture.root.join(old), fixture.root.join(new)).unwrap();

    let tree = fixture.ok(&["tree"]);
    let tree_text = tree.to_string();
    assert!(tree_text.contains(&first_root));
    assert!(tree_text.contains(&second_root));
    assert!(tree_text.contains(new));

    let log = fixture.ok(&["log", &first_root]);
    assert_eq!(log["data"]["selected"]["chain_root_commit_id"], first_root);
    assert_eq!(
        log["data"]["selected"]["project_relative_path"], old,
        "historical selection must not borrow renamed current path"
    );
    assert_eq!(log["data"]["selected"]["position"]["start"], "0");
    assert_eq!(log["data"]["selected"]["position"]["end"], "2");

    let before = fixture.ok(&["list"]);
    // list is a bounded summary now; identities come from state + links verb.
    let st = std::fs::read_to_string(fixture.root.join(".omd/state.toml")).unwrap();
    assert!(st.contains(first_root.as_str()));
    assert!(st.contains(second_root.as_str()));
    let links_before = fixture.ok(&["links", "list", "--json", "--status", "healthy"]);
    assert_eq!(links_before["data"]["total"], 1);
    assert_eq!(links_before["data"]["items"][0]["link_id"], link_id);

    let indexed = fixture.ok(&["reindex"]);
    let cache = indexed["data"]["cache_file"].as_str().unwrap();
    std::fs::remove_file(cache).unwrap();
    fixture.ok(&["reindex"]);
    let after = fixture.ok(&["list"]);
    assert_eq!(before["data"], after["data"]);
    let links_after = fixture.ok(&["links", "list", "--json", "--status", "healthy"]);
    assert_eq!(links_after["data"], links_before["data"]);
}

#[test]
fn check_aggregates_text_and_bytes_separately_with_unmarked_denominators() {
    let fixture = Fixture::new();
    fixture.write("spec/marked.md", "\u{feff}A \u{2003}中\nB");
    fixture.write("spec/unmarked.md", "CD");
    fixture.write("spec/raw.bin", b"seed");
    fixture.write("code/impl.rs", "impl");

    fixture.ok(&["init", "code/impl.rs"]);
    fixture.ok(&["import", "spec"]);
    fixture.ok(&["import", "code"]);
    fixture.ok(&["commit", "tag", "spec", "--tag", "spec"]);
    fixture.ok(&["commit", "tag", "code", "--tag", "code"]);
    fixture.ok(&[
        "commit",
        "scope_adjust",
        "spec",
        "--rule",
        "spec->code",
        "--level",
        "warn",
    ]);

    let target = fixture.ok(&[
        "commit",
        "commit",
        "code/impl.rs",
        "--range",
        "0",
        "4",
        "--reason",
        "target",
    ]);
    let target_node = format!("range:{}", root_id(&target));

    fixture.ok(&["init", "spec/marked.md"]);
    for (start, end, mode, reason) in [
        ("0", "5", "text", "text"),
        ("0", "5", "text", "overlap"),
        ("2", "2", "text", "empty"),
        ("0", "1", "byte", "byte"),
    ] {
        fixture.ok(&[
            "commit",
            "commit",
            "spec/marked.md",
            "--range",
            start,
            end,
            "--mode",
            mode,
            "--link-to",
            &target_node,
            "--reason",
            reason,
        ]);
    }

    fixture.ok(&["init", "spec/raw.bin"]);
    fixture.write("spec/raw.bin", [0xff, b' ', 0x00, b'A']);
    fixture.ok(&[
        "commit",
        "commit",
        "spec/raw.bin",
        "--range",
        "0",
        "3",
        "--mode",
        "byte",
        "--link-to",
        &target_node,
        "--reason",
        "raw",
    ]);

    let checked = fixture.ok(&["check"]);
    let rule = &checked["data"]["check"]["rules"][0];
    assert_eq!(
        rule["status"], "fail",
        "warn rule reports gap without exit failure: {checked}"
    );
    assert_eq!(
        checked["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["kind"] == "check_failed")
            .unwrap()["severity"],
        "warning"
    );
    let text = find_group(rule, "text");
    assert_eq!(text["covered"], "3");
    assert_eq!(text["total"], "6");
    let bytes = find_group(rule, "byte");
    assert_eq!(bytes["covered"], "4");
    assert_eq!(bytes["total"], "17");

    let files = rule["coverage"]["forward"]["files"].as_array().unwrap();
    let marked_text = files
        .iter()
        .find(|file| file["project_relative_path"] == "spec/marked.md" && file["unit"] == "text")
        .unwrap();
    assert_eq!(marked_text["gaps"][0]["start"], "6");
    assert_eq!(marked_text["gaps"][0]["end"], "7");
    let unmarked = files
        .iter()
        .find(|file| file["project_relative_path"] == "spec/unmarked.md")
        .unwrap();
    assert_eq!(unmarked["covered"], "0");
    assert_eq!(unmarked["total"], "2");
    let raw = files
        .iter()
        .find(|file| file["project_relative_path"] == "spec/raw.bin")
        .unwrap();
    assert_eq!(raw["unit"], "byte");
    assert_eq!(raw["gaps"][0]["start"], "3");
    assert_eq!(raw["gaps"][0]["end"], "4");

    fixture.ok(&["remove", "spec"]);
    let removed = fixture.ok(&["check"]);
    assert!(
        removed["data"]["check"]["rules"][0]["coverage"]["forward"]["groups"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let listed = fixture.ok(&["list"]);
    let total_links = listed["data"]["link_count"].as_u64().unwrap();
    assert!(total_links >= 1, "links survive remove: {total_links}");
    // The moved file is visible in state (bounded summary, no dump):
    let st = std::fs::read_to_string(fixture.root.join(".omd/state.toml")).unwrap();
    assert!(st.contains("spec/marked.md"));
    let detail = fixture.ok(&["links", "list", "--json", "--status", "healthy"]);
    assert!(detail["data"]["total"].as_u64().unwrap() >= 1);
}

#[test]
fn command_coverage_is_incomplete_without_permission_and_empty_when_collected() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    fixture.write("code.rs", "x");
    let counter = fixture.root.join("counter");
    let script = fixture.root.join("empty.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf x >> '{}'\n", counter.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    fixture.ok(&["init", "code.rs"]);
    let target = fixture.ok(&[
        "commit", "commit", "code.rs", "--range", "0", "1", "--reason", "target",
    ]);
    let target_node = format!("range:{}", root_id(&target));
    fixture.ok(&[
        "commit",
        "init",
        "virtual.out",
        "--source-type",
        "command",
        "--executable",
        script.to_str().unwrap(),
        "--args-json",
        "[]",
    ]);
    let source = fixture.ok(&[
        "--run-command=true",
        "commit",
        "commit",
        "virtual.out",
        "--range",
        "0",
        "0",
        "--link-to",
        &target_node,
        "--reason",
        "empty",
    ]);
    let source_root = root_id(&source);
    fixture.ok(&["commit", "tag", "virtual.out", "--tag", "spec"]);
    fixture.ok(&["commit", "tag", "code.rs", "--tag", "code"]);
    fixture.ok(&[
        "commit",
        "scope_adjust",
        "virtual.out",
        "--rule",
        "spec->code",
        "--level",
        "fail",
    ]);

    let count = || {
        std::fs::read(&counter)
            .map(|bytes| bytes.len())
            .unwrap_or(0)
    };
    let before_queries = count();
    fixture.ok(&["list"]);
    fixture.ok(&["tree"]);
    fixture.ok(&["log", &source_root]);
    fixture.ok(&["reindex"]);
    assert_eq!(
        count(),
        before_queries,
        "metadata queries never execute command source"
    );

    let (code, unavailable, _) = fixture.run(&["check"]);
    assert_eq!(code, 1);
    let file = &unavailable["data"]["check"]["rules"][0]["coverage"]["forward"]["files"][0];
    assert_eq!(file["status"], "incomplete");
    assert!(file["percentage"].is_null());
    assert!(
        unavailable["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|diagnostic| {
                diagnostic.get("kind").is_some()
                    && diagnostic.get("severity").is_some()
                    && diagnostic.get("message").is_some()
                    && diagnostic.get("store").is_some()
                    && diagnostic.get("node").is_some()
                    && diagnostic.get("commit_id").is_some()
            })
    );

    let before_run = count();
    let available = fixture.ok(&["--run-command=true", "check"]);
    assert_eq!(
        count(),
        before_run + 1,
        "one check performs one command acquisition"
    );
    let forward = &available["data"]["check"]["rules"][0]["coverage"]["forward"];
    assert_eq!(forward["status"], "pass");
    assert_eq!(forward["groups"][0]["covered"], "0");
    assert_eq!(forward["groups"][0]["total"], "0");
    assert_eq!(forward["groups"][0]["percentage"], 100.0);
}
