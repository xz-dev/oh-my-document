use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

fn command_position(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" | "--run-command" => index += 1,
            value if value.starts_with("--run-command=") => index += 1,
            "--root" | "--meta" | "--project" | "--encoding" | "--source-type"
            | "--source-project" | "--source-path" | "--executable" | "--args-json"
            | "--git-commit" | "--git-path" | "--expected" => index += 2,
            value if value.starts_with('-') => index += 1,
            _ => return Some(index),
        }
    }
    None
}

fn needs_expected(args: &[&str]) -> bool {
    if args.contains(&"--expected") {
        return false;
    }
    let Some(index) = command_position(args) else {
        return false;
    };
    match args[index] {
        "verify" | "check" | "log" | "tree" | "list" | "reindex" | "project" => false,
        "init" | "copy" => false,
        "note" => args.get(index + 1).is_some_and(|action| *action != "list"),
        "commit" => args.get(index + 1).is_none_or(|kind| *kind != "init"),
        _ => true,
    }
}

fn copy_context_args(args: &[&str], out: &mut Vec<String>) {
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--root" | "--meta" | "--project" | "--encoding" => {
                if let Some(value) = args.get(index + 1) {
                    out.push(args[index].to_string());
                    out.push((*value).to_string());
                }
                index += 2;
            }
            value if value.starts_with("--run-command=") => {
                out.push(value.to_string());
                index += 1;
            }
            _ => index += 1,
        }
    }
}

/// Existing regression fixtures explicitly acquire fresh observation evidence
/// before each mutation. Tests for missing/stale evidence invoke binary
/// directly instead of this helper.
pub fn with_expected(
    bin: &Path,
    cwd: &Path,
    args: &[&str],
    implicit_meta: Option<&Path>,
    home: Option<&Path>,
    config: Option<&Path>,
    cache: Option<&Path>,
) -> Vec<String> {
    if !needs_expected(args) {
        return args.iter().map(|value| (*value).to_string()).collect();
    }
    let mut verify_args = vec!["--json".to_string()];
    if let Some(meta) = implicit_meta {
        verify_args.push("--meta".into());
        verify_args.push(meta.to_string_lossy().into_owned());
    }
    copy_context_args(args, &mut verify_args);
    verify_args.push("verify".into());
    let mut command = Command::new(bin);
    command.args(&verify_args).current_dir(cwd);
    if let Some(value) = home {
        command.env("HOME", value);
    }
    if let Some(value) = config {
        command.env("OMD_CONFIG_PATH", value);
    }
    if let Some(value) = cache {
        command.env("OMD_CACHE_PATH", value);
    }
    let output = command.output().expect("run verify for expected evidence");
    let value: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return args.iter().map(|value| (*value).to_string()).collect(),
    };
    let Some(expected) = value.get("data").and_then(|data| data.get("expected")) else {
        return args.iter().map(|value| (*value).to_string()).collect();
    };
    let mut result = vec!["--expected".into(), expected.to_string()];
    result.extend(args.iter().map(|value| (*value).to_string()));
    result
}

#[allow(dead_code)]
fn _os(_: &OsStr) {}
