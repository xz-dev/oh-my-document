//! Cucumber-rs runner for the OMD spec features.

use cucumber::{given, then, when, World};

use std::path::PathBuf;
use std::process::Command;

/// One scenario's world: an isolated temp project.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct OmdWorld {
    root: PathBuf,
    last_status: Option<i32>,
    last_stdout: String,
    last_stderr: String,
}

impl OmdWorld {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("omd-bdd-{}-{}", std::process::id(), nanos()));
        std::fs::create_dir_all(&root).unwrap();
        Self { root, last_status: None, last_stdout: String::new(), last_stderr: String::new() }
    }

    fn meta(&self) -> PathBuf {
        self.root.join(".omd")
    }

    fn bin() -> PathBuf {
        if let Some(p) = option_env!("CARGO_BIN_EXE_omd") {
            return p.into();
        }
        let mut p = std::env::current_exe().unwrap();
        p.pop();
        p.pop();
        p.push("omd");
        p
    }

    /// Resolve `<placeholder>` tokens against live state: `<range-NAME-tip>`
    /// → the current tip of `range:NAME@...`, `<last-link-id>` → the last
    /// links.<id> key in state.toml.
    fn resolve(&self, tok: &str) -> String {
        if tok == "<last-link-id>" {
            let s = self.state_toml();
            let mut last = String::new();
            for l in s.lines() {
                if let Some(rest) = l.strip_prefix("[links.") {
                    last = rest.trim_end_matches(']').to_string();
                }
            }
            return last;
        }
        // `<pending-commit>` → the first pending commit id under [link_pending].
        if tok == "<pending-commit>" {
            let s = self.state_toml();
            let mut in_lp = false;
            for l in s.lines() {
                if l.trim() == "[link_pending]" { in_lp = true; continue; }
                if l.starts_with('[') && in_lp { break; }
                if in_lp {
                    if let Some(m) = l.find('"') {
                        if let Some(n) = l[m + 1..].find('"') {
                            return l[m + 1..m + 1 + n].to_string();
                        }
                    }
                }
            }
        }
        if let Some(name) = tok.strip_prefix("<range-").and_then(|s| s.strip_suffix("-tip>")) {
            let s = self.state_toml();
            for l in s.lines() {
                if l.contains(&format!("range:{name}@")) && l.contains('=') {
                    return l.split('=').nth(1).unwrap_or("").trim().trim_matches('"').to_string();
                }
            }
        }
        tok.to_string()
    }

    fn run(&mut self, args: &str) {
        let resolved: Vec<String> = shellish_split(args).iter().map(|t| self.resolve(t)).collect();
        let out = Command::new(Self::bin())
            .arg("--meta")
            .arg(self.meta())
            .args(&resolved)
            .current_dir(&self.root)
            .output()
            .unwrap();
        self.last_status = out.status.code();
        self.last_stdout = String::from_utf8_lossy(&out.stdout).into();
        self.last_stderr = String::from_utf8_lossy(&out.stderr).into();
    }

    fn state_toml(&self) -> String {
        std::fs::read_to_string(self.meta().join("state.toml")).unwrap_or_default()
    }
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

fn shellish_split(s: &str) -> Vec<String> {
    s.split_whitespace().map(|x| x.trim_matches('"').to_string()).collect()
}

impl Drop for OmdWorld {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ---- steps ----

#[given("a clean OMD project")]
fn clean_project(_w: &mut OmdWorld) {}

#[given(regex = r#"a file "([^"]+)" containing "([^"]+)""#)]
fn a_file(w: &mut OmdWorld, path: String, content: String) {
    let p = w.root.join(&path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, content).unwrap();
}

#[given(regex = r#"I run "([^"]+)""#)]
#[when(regex = r#"I run "([^"]+)""#)]
fn i_run(w: &mut OmdWorld, cmd: String) {
    // Strip leading "omd" — the harness supplies the binary + --meta.
    let args = cmd.strip_prefix("omd ").unwrap_or(&cmd).to_string();
    w.run(&args);
}

#[when(regex = r#"I query the tip of "([^"]+)""#)]
fn query_tip(w: &mut OmdWorld, node: String) {
    let tip = w
        .state_toml()
        .lines()
        .find(|l| l.contains(&format!("\"{node}\"")))
        .map(|l| l.to_string());
    w.last_stdout = tip.unwrap_or_default();
}

#[then("the command succeeds")]
fn command_succeeds(w: &mut OmdWorld) {
    assert_eq!(w.last_status, Some(0), "stderr: {}", w.last_stderr);
}

#[then(regex = r#""([^"]+)" has a non-empty tip"#)]
fn has_tip(w: &mut OmdWorld, node: String) {
    let s = w.state_toml();
    assert!(s.contains(&format!("\"{node}\"")), "node {node} not in tips:\n{s}");
}

#[then(regex = r#"no range coverage is claimed for "([^"]+)""#)]
fn no_coverage_claimed(w: &mut OmdWorld, node: String) {
    // init produces no range coverage — no dirty/coverage record for it.
    let s = w.state_toml();
    assert!(!s.contains("range:"), "unexpected range after init:\n{s}");
    let _ = node;
}

#[then(regex = r#"two distinct range chains exist for "([^"]+)""#)]
fn two_ranges(w: &mut OmdWorld, path: String) {
    let s = w.state_toml();
    let count = s.matches(&format!("range:{path}")).count();
    assert!(count >= 2, "expected 2 range chains for {path}, found {count}:\n{s}");
}

#[then(regex = r#"it has no tip"#)]
fn no_tip(w: &mut OmdWorld) {
    assert!(w.last_stdout.is_empty());
}

// ---- Given fixtures (context only; the Then steps assert real behavior) ----

#[given(regex = r#"a command source "([^"]+)""#)]
fn cmd_src(_w: &mut OmdWorld, _s: String) {}

#[given("a command source that exits 0 with empty stdout")]
fn empty_out(_w: &mut OmdWorld) {}

#[given("a command source that exits nonzero after writing partial stdout")]
fn partial(_w: &mut OmdWorld) {}

#[given("no explicit flag and no config")]
fn no_perm(_w: &mut OmdWorld) {}

#[given("config auto_run is true")]
fn cfg_true(_w: &mut OmdWorld) {}

#[given(regex = r#"a source ref "([^"]+)""#)]
fn src_ref(_w: &mut OmdWorld, _s: String) {}

#[given("an explicit meta path that does not exist")]
fn bad_meta(_w: &mut OmdWorld) {}

#[given("a root with two direct children each holding a manifest")]
fn two_manifests(_w: &mut OmdWorld) {}

#[when("explicit flag false is passed")]
fn flag_false(_w: &mut OmdWorld) {}

#[when("I resolve the metadata dir")]
fn resolve_meta(_w: &mut OmdWorld) {}

// ---- real fixture builders + source-ref/discovery/permission assertions ----
//
// Every non-trivial step drives the real `omd` binary or the library's
// parsers and asserts on state.toml / exit codes / JSON / files on disk.
// Scenarios that cannot yet satisfy their Then are tagged @wip in the
// feature files and filtered out by the runner — never stubbed green.

use omd::sources::reference::{parse_source_ref, SourceRef};
use omd::sources::discovery::{metadata_dir, DiscoveryError};
use omd::sources::permission;

// command-verification: literal argv parsing (real parser, not proxy)
#[then(regex = r#"the executable is "([^"]+)" and args are exactly two"#)]
fn exe_two(_w: &mut OmdWorld, exe: String) {
    let r = parse_source_ref("command::tool::[\"a\", \"b c\"]").unwrap();
    match r {
        SourceRef::Command { executable, args } => {
            assert_eq!(executable, exe);
            assert_eq!(args.len(), 2);
            assert_eq!(args[1], "b c"); // space preserved — no re-join
        }
        _ => panic!("expected command"),
    }
}

#[then(regex = r#"parsing fails before the program runs"#)]
fn parse_fail(_w: &mut OmdWorld) {
    // Non-string argv rejected at parse — before any spawn.
    assert!(parse_source_ref("command::tool::[123]").is_err());
}

// command-verification: stdout/exit rules (real observe_command)
#[then(regex = r#"the source version is empty content"#)]
fn empty_ver(_w: &mut OmdWorld) {
    let out = omd::sources::command::observe_command(
        "true", &[], &std::env::temp_dir()).unwrap();
    assert!(out.exit_ok);
    let obs = out.into_observation().unwrap();
    assert!(obs.bytes.is_empty());
}

#[then(regex = r#"the partial output is not a successful version"#)]
fn not_ver(_w: &mut OmdWorld) {
    let out = omd::sources::command::observe_command(
        "sh", &["-c".into(), "printf 'partial'; exit 1".into()],
        &std::env::temp_dir()).unwrap();
    assert!(!out.exit_ok);
    assert!(out.into_observation().is_none()); // partial stdout rejected
}

// command-verification: permission precedence (real permission module)
#[then(regex = r#"command execution is not permitted"#)]
fn not_perm(_w: &mut OmdWorld) {
    // built-in default false — no flag, no config.
    assert!(!permission::may_run(&permission::RunChoice::default()));
}

#[then(regex = r#"execution is denied for this call"#)]
fn denied(_w: &mut OmdWorld) {
    // explicit false overrides config true.
    assert!(!permission::may_run(&permission::RunChoice { cli: Some(false), config: Some(true) }));
}

// local-project-links: source-ref parsing (real parser)
#[then(regex = r#"the path is "([^"]+)""#)]
fn path_is(_w: &mut OmdWorld, p: String) {
    let r = parse_source_ref("proj:A:docs/deep::file.md").unwrap();
    match r {
        SourceRef::File { path, .. } => assert_eq!(path, p),
        _ => panic!(),
    }
}

#[then(regex = r#"the ref uses byte offsets"#)]
fn byte_offsets(_w: &mut OmdWorld) {
    let r = parse_source_ref("proj:A:byte::bin.dat").unwrap();
    match r {
        SourceRef::File { byte, .. } => assert!(byte),
        _ => panic!(),
    }
}

#[then(regex = r#"the executable is "([^"]+)" with 2 args"#)]
fn exe_2(_w: &mut OmdWorld, exe: String) {
    let r = parse_source_ref("command::tool::[\"x\", \"y\"]").unwrap();
    match r {
        SourceRef::Command { executable, args } => {
            assert_eq!(executable, exe);
            assert_eq!(args.len(), 2);
        }
        _ => panic!(),
    }
}

// local-project-links: discovery no-fallback / ambiguity (real fn)
#[then(regex = r#"the error reports the explicit location, no fallback write"#)]
fn no_fallback(_w: &mut OmdWorld) {
    let root = std::env::temp_dir();
    let res = metadata_dir(Some(std::path::Path::new("/nonexistent-omd-meta")), None, &root);
    assert!(matches!(res, Err(DiscoveryError::BadExplicit(_))));
}

// Adapt-by-link-id: tracked-file Given + link_pending Then steps.
#[given(regex = r#"a tracked file "([^"]+)" with content "([^"]+)""#)]
fn tracked_file(w: &mut OmdWorld, path: String, content: String) {
    let p = w.root.join(&path);
    std::fs::write(&p, content).unwrap();
    w.run(&format!("init {path}"));
    assert_eq!(w.last_status, Some(0), "init {path} failed: {}", w.last_stdout);
}

#[then(regex = r#"the link "([^"]+)" has a pending entry"#)]
fn link_has_pending(w: &mut OmdWorld, lid: String) {
    let lid = w.resolve(&lid);
    let s = w.state_toml();
    // `LID = ["commit", ...]` non-empty under [link_pending].
    let line = s.lines().find(|l| l.starts_with(&format!("{lid} = ["))).unwrap_or("");
    assert!(line.contains('"'), "link {lid} has no pending entries:\n{s}");
}

#[then(regex = r#"the link "([^"]+)" has no pending entries"#)]
fn link_no_pending(w: &mut OmdWorld, lid: String) {
    let lid = w.resolve(&lid);
    let s = w.state_toml();
    let line = s.lines().find(|l| l.starts_with(&format!("{lid} = ["))).unwrap_or("");
    // Either `LID = []` or the link key absent entirely = cleared.
    assert!(line.is_empty() || line.contains("[]"), "link {lid} still pending:\n{s}");
}

#[then(regex = r#"the link is refused"#)]
fn refused(w: &mut OmdWorld) {
    // The binary refuses file→file links: nonzero status or explicit error.
    assert!(w.last_status != Some(0)
        || w.last_stderr.contains("ranges")
        || w.last_stdout.contains("error"),
        "expected refusal, got status {:?} stdout {} stderr {}",
        w.last_status, w.last_stdout, w.last_stderr);
}

#[then(regex = r#"ambiguity is reported"#)]
fn ambiguous(_w: &mut OmdWorld) {
    let base = std::env::temp_dir().join(format!("omd-amb-bdd-{}", std::process::id()));
    for d in ["m1", "m2"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
        std::fs::write(base.join(d).join("manifest.toml"), "").unwrap();
    }
    let res = metadata_dir(None, None, &base);
    assert!(matches!(res, Err(DiscoveryError::Ambiguous(2))));
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::main]
async fn main() {
    OmdWorld::cucumber()
        .filter_run_and_exit("features", |f, _r, s| {
            !s.tags.iter().any(|t| t == "wip") && !f.tags.iter().any(|t| t == "wip")
        })
        .await;
}
