//! Group 5.4: import scope resolution — recursive walk, gitignore-style
//! include/exclude order, symlink cycle/broken detection, .omd/ importable.

use omd::sources::scope::resolve;
use std::path::PathBuf;

fn tmp(n: &str) -> PathBuf {
    let r = std::env::temp_dir().join(format!("omd-scope-{}-{}", n, std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    r
}
fn w(root: &std::path::Path, rel: &str, c: &str) {
    let f = root.join(rel);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, c).unwrap();
}
fn has(scope: &omd::sources::scope::Scope, rel: &str) -> bool {
    scope.files.iter().any(|p| p.to_string_lossy() == rel)
}

#[test]
fn recursive_includes_subdirs() {
    let r = tmp("rec");
    w(&r, "a.md", "1");
    w(&r, "sub/b.md", "2");
    w(&r, "sub/deep/c.md", "3");
    let s = resolve(&r, &[]);
    assert!(has(&s, "a.md") && has(&s, "sub/b.md") && has(&s, "sub/deep/c.md"));
}

#[test]
fn exclude_then_include_ordering() {
    let r = tmp("ord");
    w(&r, "keep.md", "1");
    w(&r, "drop.tmp", "2");
    // exclude *.tmp, then re-include drop.tmp — last match wins.
    let s = resolve(&r, &["*.tmp".into(), "!drop.tmp".into()]);
    assert!(has(&s, "keep.md"));
    assert!(has(&s, "drop.tmp"), "! re-include must win");
}

#[test]
fn exclude_drops_matching_files() {
    let r = tmp("ex");
    w(&r, "a.md", "1");
    w(&r, "b.tmp", "2");
    let s = resolve(&r, &["*.tmp".into()]);
    assert!(has(&s, "a.md") && !has(&s, "b.tmp"));
}

#[test]
fn broken_symlink_is_a_problem_not_full_coverage() {
    let r = tmp("broken");
    w(&r, "real.md", "1");
    std::os::unix::fs::symlink(r.join("gone.md"), r.join("dangling.md")).unwrap();
    let s = resolve(&r, &[]);
    assert!(
        s.problems.iter().any(|p| p.contains("broken")),
        "{:?}",
        s.problems
    );
}

#[test]
fn symlink_cycle_reported() {
    let r = tmp("cyc");
    w(&r, "a.md", "1");
    std::fs::create_dir_all(r.join("x")).unwrap();
    std::os::unix::fs::symlink(&r, r.join("x/loop")).unwrap();
    let s = resolve(&r, &[]);
    assert!(
        s.problems.iter().any(|p| p.contains("cycle")),
        "{:?}",
        s.problems
    );
}

#[test]
fn dot_omd_is_importable_not_hidden() {
    let r = tmp("omddot");
    w(&r, ".omd/state.toml", "x");
    w(&r, ".omd/commits/c.toml", "y");
    w(&r, "normal.md", "1");
    let s = resolve(&r, &[]);
    assert!(
        has(&s, ".omd/state.toml"),
        ".omd/ must be importable, not hidden"
    );
}

#[test]
fn dir_pattern_excludes_subtree() {
    let r = tmp("dirpat");
    w(&r, "keep/a.md", "1");
    w(&r, "vendor/lib.rs", "2");
    let s = resolve(&r, &["vendor/".into()]);
    assert!(has(&s, "keep/a.md") && !has(&s, "vendor/lib.rs"));
}
