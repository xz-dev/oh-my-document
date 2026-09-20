//! Group 9: command sources — fixed argv, project-root cwd, stdout-only
//! content, exit-0 requirement, execution permission precedence.

use omd::sources::command::{observe_command, parse_command_ref};
use omd::sources::permission::{RunChoice, may_run};

#[test]
fn literal_argv_boundaries_preserved() {
    // Args with empty strings, spaces, and literal `::` keep their order and
    // boundaries — never re-split.
    let (exe, argv) =
        parse_command_ref("command::mycmd::[\"\", \"has space\", \"a::b\", \"x\"]").unwrap();
    assert_eq!(exe, "mycmd");
    assert_eq!(argv, vec!["", "has space", "a::b", "x"]);
}

#[test]
fn non_string_args_rejected_before_launch() {
    // Numbers/objects in the JSON array are invalid — rejected pre-launch.
    assert!(parse_command_ref("command::c::[1,2]").is_err());
    assert!(parse_command_ref("command::c::[{\"a\":1}]").is_err());
    assert!(parse_command_ref("command::c::not-json").is_err());
}

#[test]
fn empty_argv_is_legal() {
    let (exe, argv) = parse_command_ref("command::c::[]").unwrap();
    assert_eq!(exe, "c");
    assert!(argv.is_empty());
}

#[test]
fn command_runs_in_project_root() {
    let dir = std::env::temp_dir().join(format!("omd-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = observe_command("pwd", &[], dir.as_path()).unwrap();
    assert!(out.exit_ok);
    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Real path may differ by symlink — compare canonicalized.
    let want = std::fs::canonicalize(&dir)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let gotc = std::fs::canonicalize(&got)
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(gotc, want);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn non_zero_exit_is_failure_not_content() {
    let dir = std::env::temp_dir();
    let out = observe_command("false", &[], dir.as_path()).unwrap();
    assert!(!out.exit_ok);
    // into_observation yields no new version — partial/old output is not
    // promoted to successful content.
    assert!(out.into_observation().is_none());
}

#[test]
fn empty_stdout_on_success_is_legal_empty_content() {
    let dir = std::env::temp_dir();
    let out = observe_command("true", &[], dir.as_path()).unwrap();
    assert!(out.exit_ok);
    let obs = out.into_observation().unwrap();
    assert!(obs.bytes.is_empty());
}

#[test]
fn stderr_does_not_fail_a_successful_run() {
    let dir = std::env::temp_dir();
    // `sh -c 'echo out; echo err >&2'` exits 0 with non-empty stderr.
    let out = observe_command("sh", &["-c".into(), "echo out; echo err >&2".into()], &dir).unwrap();
    assert!(out.exit_ok);
    let obs = out.into_observation().unwrap();
    assert_eq!(obs.bytes, b"out\n"); // stderr kept separate, not in content
}

#[test]
fn large_output_drains_without_deadlock() {
    // A command producing >pipe-buffer stdout must not deadlock or truncate.
    let dir = std::env::temp_dir();
    let out = observe_command(
        "sh",
        &[
            "-c".into(),
            "head -c 2000000 /dev/zero | tr '\\0' 'x'".into(),
        ],
        &dir,
    )
    .unwrap();
    assert!(out.exit_ok);
    assert_eq!(out.stdout.len(), 2_000_000);
}

#[test]
fn init_command_captures_full_stdout() {
    // init (explicit init) captures the first complete stdout as the source
    // version — same tracking flow as a file afterward.
    let dir = std::env::temp_dir();
    let out = observe_command(
        "sh",
        &["-c".into(), "printf 'line1\\nline2\\n'".into()],
        &dir,
    )
    .unwrap();
    assert!(out.exit_ok);
    let obs = out.into_observation().unwrap();
    assert_eq!(obs.bytes, b"line1\nline2\n");
}

#[test]
fn execution_permission_precedence() {
    // CLI flag > config > built-in false.
    assert!(may_run(&RunChoice {
        cli: Some(true),
        config: Some(false)
    }));
    assert!(!may_run(&RunChoice {
        cli: Some(false),
        config: Some(true)
    }));
    assert!(may_run(&RunChoice {
        cli: None,
        config: Some(true)
    }));
    assert!(!may_run(&RunChoice::default())); // built-in floor = false
}
