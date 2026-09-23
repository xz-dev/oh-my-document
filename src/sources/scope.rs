//! Import scope resolution (E-5.4): recursive walk honoring gitignore-style
//! include/exclude in order, following symlinks with cycle/broken-link
//! detection. `.omd/` is importable explicitly — never hidden. Scope feeds
//! the statistics check; it never confirms content by itself.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A resolved import scope: the in-scope files plus traversal diagnostics.
#[derive(Debug, Default)]
pub struct Scope {
    /// In-scope file paths (relative to the import root).
    pub files: Vec<PathBuf>,
    /// Diagnostics: broken links, cycles, unreadable dirs. These are
    /// diagnostics, not a marker lifecycle — they make coverage <100%.
    pub problems: Vec<String>,
}

/// Resolve the import scope under `root`. `exclude` patterns drop matching
/// paths; `include` patterns re-admit them (gitignore `!`-negation).
/// Within each list, later entries win; a path surviving is `!excluded` OR
/// `included` — include overrides exclude (the standard gitignore cascade).
/// `dir/` patterns match a subtree; bare `*.tmp` matches a filename at any
/// depth. `None` patterns include everything.
pub fn resolve(root: &Path, patterns: &[String]) -> Scope {
    // `patterns` is the ordered stream: entries prefixed `!` are includes,
    // the rest excludes. Callers pass `--exclude`s then `--include`s as `!x`.
    let mut scope = Scope::default();
    if root.is_file() {
        if let Some(name) = root.file_name() {
            let relative = PathBuf::from(name);
            if included(&relative, false, patterns) {
                scope.files.push(relative);
            }
        }
        return scope;
    }
    let mut visited: HashSet<PathBuf> = HashSet::new();
    walk(root, root, patterns, &mut visited, &mut scope);
    scope
}

fn walk(
    base: &Path,
    dir: &Path,
    patterns: &[String],
    visited: &mut HashSet<PathBuf>,
    scope: &mut Scope,
) {
    // Canonicalize to detect symlink cycles — a visited real path is not re-entered.
    match dir.canonicalize() {
        Ok(c) if !visited.insert(c.clone()) => {
            scope.problems.push(format!("cycle: {}", dir.display()));
            return;
        }
        Err(e) => {
            scope
                .problems
                .push(format!("unreadable: {}: {e}", dir.display()));
            return;
        }
        _ => {}
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            scope
                .problems
                .push(format!("unreadable: {}: {e}", dir.display()));
            return;
        }
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let rel = p.strip_prefix(base).unwrap_or(&p).to_path_buf();
        let ft = entry.file_type();
        // Broken symlink: file_type errors / canonicalize fails.
        if p.is_symlink() && p.canonicalize().is_err() {
            scope
                .problems
                .push(format!("broken link: {}", rel.display()));
            continue;
        }
        // Follow symlinks: `p.is_dir()` traverses to the target so a
        // symlink-to-dir recurses (cycle detection is the canonicalize set).
        let is_dir = matches!(ft, Ok(t) if t.is_dir()) || p.is_dir();
        if is_dir {
            if included(&rel, true, patterns) {
                walk(base, &p, patterns, visited, scope);
            }
        } else if included(&rel, false, patterns) {
            scope.files.push(rel);
        }
    }
}

/// gitignore-style: last matching pattern wins; `!` negates; `dir/` matches a
/// subtree; `*` matches a path component segment.
fn included(rel: &Path, is_dir: bool, patterns: &[String]) -> bool {
    let s = rel.to_string_lossy().replace('\\', "/");
    let mut inc = true; // default: everything is in scope
    for pat in patterns {
        let (neg, pat) = pat
            .strip_prefix('!')
            .map_or((false, pat.as_str()), |p| (true, p));
        let (dir_only, pat) = pat.strip_suffix('/').map_or((false, pat), |p| (true, p));
        if dir_only && !is_dir {
            continue;
        }
        if glob_match(pat, &s)
            || glob_match(&format!("{pat}/**"), &s)
            || s.starts_with(&format!("{pat}/"))
        {
            // bare match excludes; `!` match re-includes (gitignore polarity).
            inc = neg;
        }
    }
    inc
}

/// Minimal glob: `*` any chars in a segment, `**` any path tail, `?` one char.
fn glob_match(pat: &str, s: &str) -> bool {
    if let Some(prefix) = pat.strip_suffix("/**") {
        return s == prefix || s.starts_with(&format!("{prefix}/"));
    }
    let pn: Vec<&str> = pat.split('/').collect();
    let sn: Vec<&str> = s.split('/').collect();
    if pn.len() != sn.len() {
        // allow `*.tmp` to match a bare filename at any depth
        if pn.len() == 1 {
            return seg_match(pat, sn.last().copied().unwrap_or(""));
        }
        return false;
    }
    pn.iter().zip(sn.iter()).all(|(p, s)| seg_match(p, s))
}

fn seg_match(p: &str, s: &str) -> bool {
    // wildcard matching within one path segment
    let (mut pi, mut si) = (0usize, 0usize);
    let pb: Vec<char> = p.chars().collect();
    let sb: Vec<char> = s.chars().collect();
    let (mut star, mut ss) = (usize::MAX, 0usize);
    while si < sb.len() {
        if pi < pb.len() && (pb[pi] == '?' || pb[pi] == sb[si]) {
            pi += 1;
            si += 1;
        } else if pi < pb.len() && pb[pi] == '*' {
            star = pi;
            ss = si;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            ss += 1;
            si = ss;
        } else {
            return false;
        }
    }
    while pi < pb.len() && pb[pi] == '*' {
        pi += 1;
    }
    pi == pb.len()
}
