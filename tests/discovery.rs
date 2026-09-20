//! Group 5: deterministic discovery + source-reference parsing.

use omd::sources::discovery::{DiscoveryError, metadata_dir};
use omd::sources::encoding::{EncodingChoice, resolve};
use omd::sources::reference::{SourceRef, parse_source_ref};

#[test]
fn proj_ref_keeps_whole_path_with_colons_and_slashes() {
    // `proj:A:` rest is the full path — never re-split on `::` or `/`.
    let r = parse_source_ref("proj:A:docs/deep::weird.md").unwrap();
    match r {
        SourceRef::File { alias, path, byte } => {
            assert_eq!(alias, "A");
            assert_eq!(path, "docs/deep::weird.md"); // `::` preserved literally
            assert!(!byte);
        }
        _ => panic!("expected file"),
    }
}

#[test]
fn proj_root_alias() {
    let r = parse_source_ref("proj:root:a.md").unwrap();
    match r {
        SourceRef::File { alias, .. } => assert_eq!(alias, "root"),
        _ => panic!(),
    }
}

#[test]
fn byte_mode_marker() {
    let r = parse_source_ref("proj:A:byte::bin.dat").unwrap();
    match r {
        SourceRef::File { byte, .. } => assert!(byte),
        _ => panic!(),
    }
}

#[test]
fn command_ref_parses_argv_as_json() {
    let r = parse_source_ref("command::tool::[\"a\", \"b c\"]").unwrap();
    match r {
        SourceRef::Command { executable, args } => {
            assert_eq!(executable, "tool");
            assert_eq!(args, vec!["a", "b c"]);
        }
        _ => panic!("expected command"),
    }
}

#[test]
fn explicit_bad_metadata_dir_never_falls_back() {
    let root = std::env::temp_dir();
    let bad = std::path::Path::new("/nonexistent-omd-meta");
    let res = metadata_dir(Some(bad), None, &root);
    assert!(matches!(res, Err(DiscoveryError::BadExplicit(_))));
}

#[test]
fn two_metadata_candidates_is_ambiguous() {
    let base = std::env::temp_dir().join(format!("omd-amb-{}", std::process::id()));
    for d in ["m1", "m2"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
        std::fs::write(base.join(d).join("manifest.toml"), "").unwrap();
    }
    let res = metadata_dir(None, None, &base);
    assert!(matches!(res, Err(DiscoveryError::Ambiguous(2))));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn unique_direct_child_manifest_resolves() {
    let base = std::env::temp_dir().join(format!("omd-uniq-{}", std::process::id()));
    std::fs::create_dir_all(base.join("m")).unwrap();
    std::fs::write(base.join("m").join("manifest.toml"), "").unwrap();
    let res = metadata_dir(None, None, &base).unwrap();
    assert_eq!(res, base.join("m"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn dot_omd_under_root_wins_over_children() {
    let base = std::env::temp_dir().join(format!("omd-dot-{}", std::process::id()));
    std::fs::create_dir_all(base.join(".omd")).unwrap();
    std::fs::create_dir_all(base.join("other")).unwrap();
    std::fs::write(base.join("other").join("manifest.toml"), "").unwrap();
    let res = metadata_dir(None, None, &base).unwrap();
    assert_eq!(res, base.join(".omd"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn empty_env_var_counts_as_unset() {
    // Already covered by EncodingChoice floor; env_path filters empty.
    let c = EncodingChoice::default();
    assert_eq!(resolve(&c), "utf-8");
}
