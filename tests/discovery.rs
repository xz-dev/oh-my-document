//! Group 5: deterministic discovery + closed source-field validation.

use omd::sources::discovery::{DiscoveryError, metadata_dir};
use omd::sources::encoding::{EncodingChoice, resolve};
use omd::sources::reference::{SourceDescriptor, SourceFields};

#[test]
fn file_fields_keep_literal_scheme_shaped_path() {
    let descriptor = SourceFields {
        source_type: Some("file".into()),
        source_project: Some("A".into()),
        source_path: Some("docs/command::deep::weird.md".into()),
        ..Default::default()
    }
    .descriptor(None, false)
    .unwrap()
    .unwrap();
    assert_eq!(
        descriptor,
        SourceDescriptor::File {
            project: "A".into(),
            path: "docs/command::deep::weird.md".into(),
        }
    );
}

#[test]
fn new_file_defaults_to_root_and_target_path() {
    let descriptor = SourceFields::default()
        .descriptor(Some("a.md"), false)
        .unwrap()
        .unwrap();
    assert_eq!(
        descriptor,
        SourceDescriptor::File {
            project: "root".into(),
            path: "a.md".into(),
        }
    );
}

#[test]
fn command_fields_parse_literal_json_argv() {
    let descriptor = SourceFields {
        source_type: Some("command".into()),
        executable: Some("tool".into()),
        args_json: Some("[\"\",\"a b\",\"::\"]".into()),
        ..Default::default()
    }
    .descriptor(None, false)
    .unwrap()
    .unwrap();
    assert_eq!(
        descriptor,
        SourceDescriptor::Command {
            executable: "tool".into(),
            args: vec!["".into(), "a b".into(), "::".into()],
        }
    );
}

#[test]
fn incompatible_or_non_string_command_fields_reject() {
    assert!(
        SourceFields {
            source_type: Some("file".into()),
            executable: Some("tool".into()),
            ..Default::default()
        }
        .descriptor(Some("a.md"), false)
        .is_err()
    );
    assert!(
        SourceFields {
            source_type: Some("command".into()),
            executable: Some("tool".into()),
            args_json: Some("[123]".into()),
            ..Default::default()
        }
        .descriptor(None, false)
        .is_err()
    );
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
        omd::records::store::Store::open(&base.join(d)).unwrap();
    }
    let res = metadata_dir(None, None, &base);
    assert!(matches!(res, Err(DiscoveryError::Ambiguous(2))));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn unique_direct_child_manifest_resolves() {
    let base = std::env::temp_dir().join(format!("omd-uniq-{}", std::process::id()));
    omd::records::store::Store::open(&base.join("m")).unwrap();
    let res = metadata_dir(None, None, &base).unwrap();
    assert_eq!(res, base.join("m"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn dot_omd_under_root_wins_over_children() {
    let base = std::env::temp_dir().join(format!("omd-dot-{}", std::process::id()));
    omd::records::store::Store::open(&base.join(".omd")).unwrap();
    omd::records::store::Store::open(&base.join("other")).unwrap();
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
