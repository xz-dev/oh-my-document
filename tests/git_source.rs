//! Group 10.1: exact-commit raw Git recovery from a mapped local repo.

use omd::sources::git::{GitRef, read_blob};
use omd::sources::reference::{SourceDescriptor, SourceFields};
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit(dir: &std::path::Path, message: &str) -> String {
    git(dir, &["add", "."]);
    git(
        dir,
        &["-c", "commit.gpgsign=false", "commit", "-qm", message],
    );
    String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string()
}

fn make_repo() -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    git(repo.path(), &["config", "user.email", "t@t"]);
    git(repo.path(), &["config", "user.name", "t"]);
    std::fs::write(repo.path().join("f.txt"), "historical content X").unwrap();
    let id = commit(repo.path(), "c1");
    (repo, id)
}

fn reference(repo: &std::path::Path, commit: &str, path: &str) -> GitRef {
    GitRef {
        repo: repo.to_path_buf(),
        commit: commit.to_string(),
        path: path.to_string(),
    }
}

#[test]
fn exact_commit_blob_read() {
    let (repo, c1) = make_repo();
    assert_eq!(
        read_blob(&reference(repo.path(), &c1, "f.txt")).unwrap(),
        b"historical content X"
    );
}

#[test]
fn git_paths_are_literal_not_pathspecs() {
    let (repo, _) = make_repo();
    let name = ":(glob)*.txt";
    std::fs::write(repo.path().join(name), "literal git path").unwrap();
    let commit = commit(repo.path(), "literal path");
    assert_eq!(
        read_blob(&reference(repo.path(), &commit, name)).unwrap(),
        b"literal git path"
    );
}

#[test]
fn head_movement_does_not_change_observation() {
    let (repo, c1) = make_repo();
    std::fs::write(repo.path().join("f.txt"), "different Y").unwrap();
    commit(repo.path(), "c2");
    assert_eq!(
        read_blob(&reference(repo.path(), &c1, "f.txt")).unwrap(),
        b"historical content X"
    );
}

#[test]
fn floating_ref_name_rejected() {
    let (repo, _) = make_repo();
    assert!(read_blob(&reference(repo.path(), "HEAD", "f.txt")).is_err());
}

#[test]
fn missing_object_reports_unobtainable() {
    let (repo, c1) = make_repo();
    assert!(read_blob(&reference(repo.path(), &c1, "gone.txt")).is_err());
}

#[cfg(unix)]
#[test]
fn in_commit_symlink_resolves_relative_to_its_parent() {
    let (repo, _) = make_repo();
    std::fs::create_dir(repo.path().join("dir")).unwrap();
    std::os::unix::fs::symlink("../f.txt", repo.path().join("dir/link.txt")).unwrap();
    let c2 = commit(repo.path(), "symlink");
    assert_eq!(
        read_blob(&reference(repo.path(), &c2, "dir/link.txt")).unwrap(),
        b"historical content X"
    );
}

#[cfg(unix)]
#[test]
fn historical_symlink_break_and_cycle_fail() {
    let (repo, _) = make_repo();
    std::os::unix::fs::symlink("missing", repo.path().join("broken")).unwrap();
    std::os::unix::fs::symlink("b", repo.path().join("a")).unwrap();
    std::os::unix::fs::symlink("a", repo.path().join("b")).unwrap();
    std::os::unix::fs::symlink("grow/tail", repo.path().join("grow")).unwrap();
    let c2 = commit(repo.path(), "bad links");
    assert!(read_blob(&reference(repo.path(), &c2, "broken")).is_err());
    assert!(read_blob(&reference(repo.path(), &c2, "a")).is_err());
    assert!(
        read_blob(&reference(repo.path(), &c2, "grow/f.txt"))
            .unwrap_err()
            .to_string()
            .contains("symlink cycle")
    );
}

#[test]
fn git_read_blob_follows_historical_directory_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    git(repo, &["init", "-q"]);
    std::fs::create_dir_all(repo.join("real")).unwrap();
    std::fs::write(repo.join("real/leaf.md"), "historical").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("real", repo.join("alias")).unwrap();
    git(repo, &["add", "real/leaf.md", "alias"]);
    let commit_id = commit(repo, "history");
    let through_alias = omd::sources::git::read_blob(&omd::sources::git::GitRef {
        repo: repo.to_path_buf(),
        commit: commit_id.clone(),
        path: "alias/leaf.md".into(),
    })
    .unwrap();
    let direct = omd::sources::git::read_blob(&omd::sources::git::GitRef {
        repo: repo.to_path_buf(),
        commit: commit_id,
        path: "real/leaf.md".into(),
    })
    .unwrap();
    assert_eq!(through_alias, direct);
}

#[cfg(unix)]
#[test]
fn git_read_blob_follows_root_directory_symlink_and_repeated_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    git(repo, &["init", "-q"]);
    std::fs::write(repo.join("leaf.md"), "historical").unwrap();
    std::os::unix::fs::symlink(".", repo.join("alias")).unwrap();
    let commit_id = commit(repo, "root alias");
    for path in ["alias/leaf.md", "alias/alias/leaf.md"] {
        let through_alias = read_blob(&GitRef {
            repo: repo.to_path_buf(),
            commit: commit_id.clone(),
            path: path.into(),
        })
        .unwrap();
        assert_eq!(through_alias, b"historical");
    }
    assert_eq!(
        read_blob(&GitRef {
            repo: repo.to_path_buf(),
            commit: commit_id,
            path: "leaf.md".into(),
        })
        .unwrap(),
        b"historical"
    );
}

#[test]
fn git_fields_require_complete_exact_commit() {
    let descriptor = SourceFields {
        source_type: Some("git".into()),
        source_project: Some("repo".into()),
        git_commit: Some("a".repeat(40)),
        git_path: Some("f.txt".into()),
        ..Default::default()
    }
    .descriptor(None, false)
    .unwrap()
    .unwrap();
    assert_eq!(
        descriptor,
        SourceDescriptor::Git {
            project: "repo".into(),
            commit: "a".repeat(40),
            path: "f.txt".into(),
        }
    );
    let default_root = SourceFields {
        source_type: Some("git".into()),
        git_commit: Some("a".repeat(40)),
        git_path: Some("f.txt".into()),
        ..Default::default()
    }
    .descriptor(None, false)
    .unwrap()
    .unwrap();
    assert_eq!(
        default_root,
        SourceDescriptor::Git {
            project: "root".into(),
            commit: "a".repeat(40),
            path: "f.txt".into(),
        }
    );
    assert!(
        SourceFields {
            source_type: Some("git".into()),
            source_project: Some("repo".into()),
            git_commit: Some("HEAD".into()),
            git_path: Some("f.txt".into()),
            ..Default::default()
        }
        .descriptor(None, false)
        .is_err()
    );
}
