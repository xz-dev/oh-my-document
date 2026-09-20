//! Group 10.1: git::<JSON> exact-commit blob reads from a local repo.

use omd::sources::git::{parse_git_ref, read_blob};
use omd::sources::reference::{parse_source_ref, SourceRef};
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
}

fn make_repo() -> (std::path::PathBuf, String) {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let r = std::env::temp_dir().join(format!("omd-git-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q"]);
    git(&r, &["config", "user.email", "t@t"]);
    git(&r, &["config", "user.name", "t"]);
    std::fs::write(r.join("f.txt"), "historical content X").unwrap();
    git(&r, &["add", "."]);
    git(&r, &["-c", "commit.gpgsign=false", "commit", "-qm", "c1"]);
    let c1 = String::from_utf8(
        Command::new("git").arg("-C").arg(&r).args(["rev-parse", "HEAD"]).output().unwrap().stdout
    ).unwrap().trim().to_string();
    (r, c1)
}

#[test]
fn exact_commit_blob_read() {
    let (r, c1) = make_repo();
    let g = parse_git_ref(&format!("git::{{\"repo\":\"{}\",\"commit\":\"{}\",\"path\":\"f.txt\"}}", r.display(), c1)).unwrap();
    let blob = read_blob(&g).unwrap();
    assert_eq!(blob, b"historical content X");
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn head_movement_does_not_change_observation() {
    let (r, c1) = make_repo();
    // Move HEAD to a new commit with different content.
    std::fs::write(r.join("f.txt"), "different Y").unwrap();
    git(&r, &["add", "."]); git(&r, &["-c", "commit.gpgsign=false", "commit", "-qm", "c2"]);
    // Reading the recorded commit still yields the OLD content.
    let g = parse_git_ref(&format!("git::{{\"repo\":\"{}\",\"commit\":\"{}\",\"path\":\"f.txt\"}}", r.display(), c1)).unwrap();
    assert_eq!(read_blob(&g).unwrap(), b"historical content X");
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn floating_ref_name_rejected() {
    let r = std::env::temp_dir();
    let res = parse_git_ref(&format!("git::{{\"repo\":\"{}\",\"commit\":\"HEAD\",\"path\":\"f.txt\"}}", r.display()));
    assert!(res.is_err(), "HEAD must be rejected — not an exact id");
}

#[test]
fn missing_object_reports_unobtainable() {
    let (r, c1) = make_repo();
    // A path that doesn't exist at that commit.
    let g = parse_git_ref(&format!("git::{{\"repo\":\"{}\",\"commit\":\"{}\",\"path\":\"gone.txt\"}}", r.display(), c1)).unwrap();
    assert!(read_blob(&g).is_err(), "missing object must not substitute");
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn in_commit_symlink_resolves_within_commit() {
    let (r, c1) = make_repo();
    std::os::unix::fs::symlink("f.txt", r.join("link.txt")).unwrap();
    git(&r, &["add", "."]); git(&r, &["-c", "commit.gpgsign=false", "commit", "-qm", "c2"]);
    let c2 = String::from_utf8(
        Command::new("git").arg("-C").arg(&r).args(["rev-parse", "HEAD"]).output().unwrap().stdout
    ).unwrap().trim().to_string();
    let g = parse_git_ref(&format!("git::{{\"repo\":\"{}\",\"commit\":\"{}\",\"path\":\"link.txt\"}}", r.display(), c2)).unwrap();
    let blob = read_blob(&g).unwrap();
    assert_eq!(blob, b"historical content X", "symlink must resolve in-commit");
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn git_ref_parses_in_source_ref() {
    let s = parse_source_ref("git::{\"repo\":\"/r\",\"commit\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"path\":\"f.txt\"}").unwrap();
    match s {
        SourceRef::Git { commit, .. } => assert_eq!(commit.len(), 40),
        _ => panic!("expected git"),
    }
}
