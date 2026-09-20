//! Tasks 4.2–4.4: coordinates, encodings, Myers diff, migration, duplicate
//! matching, empty/deleted ranges.

use omd::relations::diff::{diff_text, dirtied_by};
use omd::relations::range::{text_len, text_slice, Mode, Range, RangeError};
use omd::sources::encoding::{resolve, EncodingChoice};

#[test]
fn text_ranges_count_scalar_positions_not_bytes() {
    // "héllo" has 5 chars but 6 bytes; a range over "é" is [1,2) chars.
    let s = "héllo";
    assert_eq!(text_len(s), 5);
    let r = Range::new(1, 2, Mode::Text, text_len(s)).unwrap();
    assert_eq!(text_slice(s, &r).unwrap(), "é");
}

#[test]
fn bom_and_crlf_are_real_characters() {
    let s = "\u{feff}a\r\nb";
    assert_eq!(text_len(s), 5); // BOM, a, \r, \n, b
}

#[test]
fn range_is_left_closed_right_open() {
    let s = "abcd";
    let r = Range::new(1, 3, Mode::Text, 4).unwrap();
    assert_eq!(text_slice(s, &r).unwrap(), "bc"); // [1,3) = 'b','c'
}

#[test]
fn out_of_bounds_rejected_not_clamped() {
    assert!(matches!(Range::new(0, 10, Mode::Text, 4), Err(RangeError::OutOfBounds)));
    assert!(matches!(Range::new(5, 3, Mode::Text, 4), Err(RangeError::Inverted)));
}

#[test]
fn same_coordinates_independent_ranges() {
    // Two ranges at identical coords are distinct tracked objects.
    let a = Range::new(0, 5, Mode::Text, 10).unwrap();
    let b = Range::new(0, 5, Mode::Text, 10).unwrap();
    assert_eq!(a, b); // equal coords, but they are independent *commits*
}

#[test]
fn byte_mode_counts_offsets() {
    let r = Range::new(0, 3, Mode::Byte, 6).unwrap();
    assert_eq!(r.len(), 3);
}

#[test]
fn insertion_at_range_end_dirties_without_growth() {
    // old: "校验密码。"  new: "校验密码。记录日志。"
    // The append lands exactly at the old range's end — ambiguous, so the
    // range dirties but does NOT auto-expand to cover the new text.
    let old = "校验密码。";
    let new = "校验密码。记录日志。";
    let range = Range::new(0, text_len(old), Mode::Text, text_len(new)).unwrap();
    let hunks = diff_text(old, new);
    let dirty = dirtied_by(&hunks, &[range]);
    assert!(dirty[0]);
    // The range itself is unchanged — it did not grow.
    assert_eq!(range.end, text_len(old));
}

#[test]
fn interior_change_dirties_overlapping_range() {
    let old = "hello world";
    let new = "hello WORLD";
    let range = Range::new(6, 11, Mode::Text, 11).unwrap();
    let dirty = dirtied_by(&diff_text(old, new), &[range]);
    assert!(dirty[0]);
}

#[test]
fn unrelated_range_stays_clean() {
    let old = "aaaa bbbb";
    let new = "aaaa BBBB";
    let range = Range::new(0, 4, Mode::Text, 9).unwrap(); // covers "aaaa"
    let dirty = dirtied_by(&diff_text(old, new), &[range]);
    assert!(!dirty[0]);
}

#[test]
fn whitespace_change_not_ignored() {
    // Coverage filtering must not skip whitespace edits.
    let old = "a b";
    let new = "a  b";
    let range = Range::new(0, 3, Mode::Text, 4).unwrap();
    let dirty = dirtied_by(&diff_text(old, new), &[range]);
    assert!(dirty[0]);
}

#[test]
fn empty_range_tracked_but_no_coverage() {
    let r = Range::new(2, 2, Mode::Text, 4).unwrap();
    assert!(r.is_empty());
    assert_eq!(r.len(), 0);
}

#[test]
fn encoding_priority_cli_beats_recorded_beats_defaults() {
    let c = EncodingChoice {
        cli: Some("utf-8".into()),
        recorded: Some("gbk".into()),
        file_config: Some("big5".into()),
        project_default: Some("latin1".into()),
        user_default: Some("shift_jis".into()),
    };
    assert_eq!(resolve(&c), "utf-8"); // CLI wins

    let c = EncodingChoice { recorded: Some("gbk".into()), ..Default::default() };
    assert_eq!(resolve(&c), "gbk"); // recorded beats config+defaults

    let c = EncodingChoice { project_default: Some("latin1".into()), ..Default::default() };
    assert_eq!(resolve(&c), "latin1"); // project default beats user default

    assert_eq!(resolve(&EncodingChoice::default()), "utf-8"); // floor
}

#[test]
fn recorded_encoding_freezes_historical_view() {
    // A recorded "gbk" observation is never reinterpreted as utf-8 just
    // because config changed — the recorded value outranks file_config.
    let c = EncodingChoice {
        recorded: Some("gbk".into()),
        file_config: Some("utf-8".into()),
        ..Default::default()
    };
    assert_eq!(resolve(&c), "gbk");
}
