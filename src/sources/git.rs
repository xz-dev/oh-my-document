//! Exact local Git object recovery.
//!
//! Only a complete hexadecimal commit object id plus literal in-commit path
//! is accepted. Plumbing runs with lazy fetching disabled and never consults
//! worktree, index, textconv, filters, refs, or remotes.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use super::SourceError;

#[derive(Debug, Clone)]
pub struct GitRef {
    pub repo: PathBuf,
    pub commit: String,
    pub path: String,
}

pub fn read_blob(gitref: &GitRef) -> Result<Vec<u8>, SourceError> {
    if !matches!(gitref.commit.len(), 40 | 64)
        || !gitref.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(SourceError::Invalid(
            "Git commit must be a complete 40- or 64-hex object id".into(),
        ));
    }
    let kind = git(gitref, &["cat-file", "-t", &gitref.commit])?;
    if kind != b"commit\n" {
        return Err(SourceError::Unavailable(
            "Git object is missing or is not a commit".into(),
        ));
    }
    let mut active_links = BTreeSet::new();
    let path = normalize_repo_path(Path::new(&gitref.path))?;
    let components: Vec<String> = path.split('/').map(str::to_string).collect();
    let (_, object_type, bytes) = resolve_path(gitref, Vec::new(), &components, &mut active_links)?;
    if object_type != "blob" {
        return Err(SourceError::Unavailable(format!(
            "Git path is not a blob: {path}"
        )));
    }
    Ok(bytes)
}

fn resolve_path(
    gitref: &GitRef,
    mut resolved: Vec<String>,
    remaining: &[String],
    active_links: &mut BTreeSet<String>,
) -> Result<(Vec<String>, String, Vec<u8>), SourceError> {
    let Some((component, rest)) = remaining.split_first() else {
        return Ok((resolved, "tree".into(), Vec::new()));
    };
    resolved.push(component.clone());
    let path = resolved.join("/");
    let (mode, object_type, bytes) = read_entry(gitref, &path)?;
    if mode != "120000" {
        if rest.is_empty() {
            return Ok((resolved, object_type, bytes));
        }
        if object_type != "tree" {
            return Err(SourceError::Unavailable(format!(
                "Git path is not a tree: {path}"
            )));
        }
        return resolve_path(gitref, resolved, rest, active_links);
    }
    if !active_links.insert(path.clone()) {
        return Err(SourceError::Unavailable(
            "historical Git symlink cycle".into(),
        ));
    }
    let target = std::str::from_utf8(&bytes)
        .map_err(|_| SourceError::Unavailable("Git symlink target is not UTF-8".into()))?;
    if target.contains('\0') || Path::new(target).is_absolute() {
        return Err(SourceError::Unavailable(
            "historical Git symlink escapes commit tree".into(),
        ));
    }
    resolved.pop();
    let parent = resolved.join("/");
    let target = normalize_repo_path_allow_root(&Path::new(&parent).join(target), true)?;
    let target_components: Vec<String> = if target.is_empty() {
        Vec::new()
    } else {
        target.split('/').map(str::to_string).collect()
    };
    // A link stays active only while its target is resolved. The caller's
    // suffix may legitimately traverse that same link again afterwards.
    let entry = resolve_path(gitref, Vec::new(), &target_components, active_links)?;
    active_links.remove(&path);
    if rest.is_empty() {
        return Ok(entry);
    }
    let (target_path, object_type, _) = entry;
    if object_type != "tree" {
        return Err(SourceError::Unavailable(format!(
            "Git path is not a tree: {path}"
        )));
    }
    resolve_path(gitref, target_path, rest, active_links)
}

fn read_entry(gitref: &GitRef, path: &str) -> Result<(String, String, Vec<u8>), SourceError> {
    let literal = format!(":(literal){path}");
    let tree = git(
        gitref,
        &[
            "ls-tree",
            "--full-tree",
            "-z",
            &gitref.commit,
            "--",
            &literal,
        ],
    )?;
    let entry = tree
        .split(|byte| *byte == 0)
        .find(|entry| !entry.is_empty())
        .ok_or_else(|| SourceError::Unavailable(format!("Git path is missing: {path}")))?;
    let tab = entry
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or_else(|| SourceError::Unavailable("invalid git ls-tree output".into()))?;
    let header = std::str::from_utf8(&entry[..tab])
        .map_err(|_| SourceError::Unavailable("invalid git ls-tree output".into()))?;
    let mut fields = header.split_whitespace();
    let mode = fields.next().unwrap_or("").to_string();
    let object_type = fields.next().unwrap_or("").to_string();
    if !matches!(object_type.as_str(), "blob" | "tree") {
        return Err(SourceError::Unavailable(format!(
            "Git path has unsupported object type: {path}"
        )));
    }
    let bytes = if object_type == "blob" {
        let spec = format!("{}:{path}", gitref.commit);
        git(gitref, &["cat-file", "blob", &spec])?
    } else {
        Vec::new()
    };
    Ok((mode, object_type, bytes))
}

pub(crate) fn normalize_repo_path(path: &Path) -> Result<String, SourceError> {
    normalize_repo_path_allow_root(path, false)
}

fn normalize_repo_path_allow_root(path: &Path, allow_root: bool) -> Result<String, SourceError> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(SourceError::Unavailable(
                        "historical Git symlink escapes commit tree".into(),
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(SourceError::Unavailable(
                    "Git path must be repository-relative".into(),
                ));
            }
        }
    }
    if parts.is_empty() && !allow_root {
        return Err(SourceError::Unavailable("Git path is empty".into()));
    }
    Ok(parts.join("/"))
}

fn git(gitref: &GitRef, args: &[&str]) -> Result<Vec<u8>, SourceError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&gitref.repo)
        .args(args)
        .env("GIT_NO_LAZY_FETCH", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(SourceError::Io)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(SourceError::Unavailable(stderr.trim_end().to_string()))
    }
}
