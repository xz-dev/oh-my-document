//! git:: sources — read exact-commit blobs from a local repository (E-10).
//!
//! `git::<JSON>` references carry `{repo, commit, path}`: the repo location,
//! a full exact Git commit ID (a floating ref name never substitutes), and
//! the in-commit file path. We read the complete raw blob via `git cat-file`
//! — read-only plumbing, never textconv/filter, never diff/index/status,
//! never an implicit fetch, clone, commit, or worktree switch.
//!
//! In-commit symlinks resolve within the same commit: a link's blob holds
//! its target path; we follow it (bounded) and report broken links / cycles
//! as diagnostics — never fall back to the worktree.

use std::path::PathBuf;
use std::process::Command;

use super::SourceError;

/// A parsed `git::{repo,commit,path}` reference.
#[derive(Debug, Clone)]
pub struct GitRef {
    pub repo: PathBuf,
    /// Full exact commit id (40-hex). Floating ref names are rejected.
    pub commit: String,
    /// Path inside that commit's tree.
    pub path: String,
}

/// Parse `git::<JSON>` — `{"repo": "...", "commit": "<40-hex>", "path": "..."}`.
pub fn parse_git_ref(s: &str) -> Result<GitRef, SourceError> {
    let json = s.strip_prefix("git::").ok_or(SourceError::Command)?;
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| SourceError::Command)?;
    let repo = v
        .get("repo")
        .and_then(|x| x.as_str())
        .ok_or(SourceError::Command)?;
    let commit = v
        .get("commit")
        .and_then(|x| x.as_str())
        .ok_or(SourceError::Command)?;
    let path = v
        .get("path")
        .and_then(|x| x.as_str())
        .ok_or(SourceError::Command)?;
    // Exact commit id: 40-hex only — no branch/tag/short/ref name.
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SourceError::Command);
    }
    Ok(GitRef {
        repo: PathBuf::from(repo),
        commit: commit.into(),
        path: path.into(),
    })
}

/// Read the raw blob for `gitref` at the exact commit. `git cat-file blob
/// <commit>:<path>` — the complete raw object, no filtering. In-commit
/// symlinks resolve within the commit (bounded depth); a broken link or
/// cycle is a diagnostic, never a worktree fallback.
pub fn read_blob(gitref: &GitRef) -> Result<Vec<u8>, SourceError> {
    read_blob_depth(gitref, 0)
}

fn read_blob_depth(gitref: &GitRef, depth: usize) -> Result<Vec<u8>, SourceError> {
    if depth > 8 {
        return Err(SourceError::Command); // symlink cycle / too deep
    }
    let out = Command::new("git")
        .args(["-C"])
        .arg(&gitref.repo)
        .args([
            "cat-file",
            "blob",
            &format!("{}:{}", gitref.commit, gitref.path),
        ])
        .output()
        .map_err(|_| SourceError::Command)?;
    if !out.status.success() {
        // Object/path missing at that commit — report unobtainable, never
        // substitute worktree content or another commit.
        return Err(SourceError::Command);
    }
    // If the entry is a symlink, its blob is the target path — follow it
    // within the same commit.
    if is_symlink_entry(gitref)? {
        let target = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let mut nxt = gitref.clone();
        nxt.path = target;
        return read_blob_depth(&nxt, depth + 1);
    }
    Ok(out.stdout)
}

/// Is `<commit>:<path>` a symlink entry (mode 120000)?
fn is_symlink_entry(gitref: &GitRef) -> Result<bool, SourceError> {
    let out = Command::new("git")
        .args(["-C"])
        .arg(&gitref.repo)
        .args(["ls-tree", &gitref.commit, "--", &gitref.path])
        .output()
        .map_err(|_| SourceError::Command)?;
    let line = String::from_utf8_lossy(&out.stdout);
    Ok(line.starts_with("120000"))
}
