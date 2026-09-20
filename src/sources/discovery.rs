//! Discovery: config/cache paths, project root, and metadata dir (E-5/5.1).
//!
//! Strict precedence, no Git-based guessing, explicit-wrong-path never falls
//! back:
//!   * config/cache: OMD_CONFIG_PATH / OMD_CACHE_PATH, then XDG/platform
//!   * project root: explicit --root > deepest registered root containing cwd
//!     > nearest upward OMD-recognizable dir — never Git
//!   * metadata: explicit path > local mapping > `<root>/.omd/` > a unique
//!     manifest in a direct child dir — multiple same-priority candidates is
//!     an ambiguity error, never a traversal-order pick

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("explicit path is wrong: {0}")]
    BadExplicit(String),
    #[error("ambiguous metadata dir: {0} candidates")]
    Ambiguous(usize),
    #[error("no project root found")]
    NoRoot,
}

/// Config dir: `OMD_CONFIG_PATH` honored first (empty = unset), then XDG
/// `~/.config/omd`, then `~/.omd` platform fallback. Relative paths resolve
/// against the calling cwd.
pub fn config_dir(cwd: &Path) -> PathBuf {
    env_path("OMD_CONFIG_PATH", cwd)
        .or_else(xdg_config)
        .unwrap_or_else(|| PathBuf::from(".omd"))
}

/// Cache dir: `OMD_CACHE_PATH` first, then XDG `~/.cache/omd`, then fallback.
pub fn cache_dir(cwd: &Path) -> PathBuf {
    env_path("OMD_CACHE_PATH", cwd)
        .or_else(xdg_cache)
        .unwrap_or_else(|| PathBuf::from(".omd-cache"))
}

fn env_path(var: &str, cwd: &Path) -> Option<PathBuf> {
    let v = std::env::var(var).ok().filter(|s| !s.is_empty())?;
    let p = PathBuf::from(v);
    Some(if p.is_absolute() { p } else { cwd.join(p) })
}

fn xdg_config() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|d| PathBuf::from(d).join("omd"))
        .or_else(|| home().map(|h| h.join(".config").join("omd")))
}

fn xdg_cache() -> Option<PathBuf> {
    std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|d| PathBuf::from(d).join("omd"))
        .or_else(|| home().map(|h| h.join(".cache").join("omd")))
}

fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Resolve the metadata dir by precedence. `explicit` is a `--meta`/`--root`
/// style path the caller passed; a wrong explicit path is an error, never a
/// fallback. `registered` is a caller-resolved mapping. Then `<root>/.omd/`,
/// then the *unique* manifest-bearing direct child of `root`.
pub fn metadata_dir(
    explicit: Option<&Path>,
    registered: Option<PathBuf>,
    root: &Path,
) -> Result<PathBuf, DiscoveryError> {
    if let Some(p) = explicit {
        if !p.exists() {
            return Err(DiscoveryError::BadExplicit(p.display().to_string()));
        }
        return Ok(p.to_path_buf());
    }
    if let Some(p) = registered {
        return Ok(p);
    }
    let dot = root.join(".omd");
    if dot.exists() {
        return Ok(dot);
    }
    // Direct children of root that look like a metadata dir (have a manifest
    // marker file). Unique wins; multiple is an ambiguity error.
    let mut cands = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && p.join("manifest.toml").exists() {
                cands.push(p);
            }
        }
    }
    match cands.len() {
        0 => Err(DiscoveryError::NoRoot),
        1 => Ok(cands.remove(0)),
        n => Err(DiscoveryError::Ambiguous(n)),
    }
}
