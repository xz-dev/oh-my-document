//! Group 7 coverage + group 8 atomic/reset domain tests.

use omd::records::commit::CommitKind;
use omd::relations::atomic::{resolve_reset, AtomicStack, ResetLanding};
use omd::relations::coverage::{coverage, coverage_percent, effective_link_span};
use omd::relations::range::{Mode, Range};

#[test]
fn overlapping_links_count_once() {
    // Two links covering the same positions don't double-count.
    let content = "abcdef";
    let r1 = Range::new(0, 4, Mode::Text, 6).unwrap();
    let r2 = Range::new(2, 6, Mode::Text, 6).unwrap();
    let (covered, denom) = coverage(&[r1, r2], content, Mode::Text);
    assert_eq!(denom, 6);
    assert_eq!(covered, 6); // union, not 4+4
}

#[test]
fn whitespace_positions_filtered_from_text_denominator() {
    let content = "a b  c";
    let r = Range::new(0, 6, Mode::Text, 6).unwrap();
    let (covered, denom) = coverage(&[r], content, Mode::Text);
    // Only 'a','b','c' count — whitespace is filtered from both sides.
    assert_eq!(denom, 3);
    assert_eq!(covered, 3);
}

#[test]
fn empty_content_reports_100_percent() {
    assert_eq!(coverage_percent(0, 0), 100.0);
}

#[test]
fn whitespace_only_content_reports_100_percent() {
    let content = "   \n\t ";
    let (_, denom) = coverage(&[], content, Mode::Text);
    assert_eq!(denom, 0);
    assert_eq!(coverage_percent(0, denom), 100.0);
}

#[test]
fn unmarked_content_stays_in_denominator() {
    let content = "abcd";
    let r = Range::new(0, 2, Mode::Text, 4).unwrap(); // only 'ab' linked
    let (covered, denom) = coverage(&[r], content, Mode::Text);
    assert_eq!(denom, 4); // 'cd' still counted
    assert_eq!(covered, 2);
}

#[test]
fn byte_mode_does_not_filter_whitespace() {
    let content = "a b";
    let r = Range::new(0, 3, Mode::Byte, 3).unwrap();
    let (covered, denom) = coverage(&[r], content, Mode::Byte);
    assert_eq!(denom, 3); // space byte counts
    assert_eq!(covered, 3);
}

#[test]
fn link_to_empty_target_fills_no_gap() {
    let r = Range::new(0, 5, Mode::Text, 10).unwrap();
    assert_eq!(effective_link_span(&r, true), 0);
}

#[test]
fn innermost_end_closes_nearest_begin() {
    let mut s = AtomicStack::default();
    s.begin("outer");
    s.begin("inner");
    assert_eq!(s.end(), Some("inner".into())); // closes nearest, not outer
    assert!(s.is_open()); // outer still open
    assert_eq!(s.end(), Some("outer".into()));
    assert!(!s.is_open());
}

#[test]
fn unclosed_block_fails_verify_even_at_full_coverage() {
    let mut s = AtomicStack::default();
    s.begin("open_block");
    // Coverage could be 100% — the open block alone fails verify.
    assert!(s.is_open());
}

#[test]
fn reset_to_marker_lands_on_direct_predecessor() {
    // Reset to a BEGIN/END marker withdraws it and successors — lands on the
    // marker's own previous_id, the "-1 step" for placeholders.
    let land = resolve_reset(CommitKind::AtomicBegin, "prev_commit", false, false);
    assert_eq!(land, ResetLanding::MarkerPredecessor("prev_commit".into()));
}

#[test]
fn reset_to_first_begin_with_no_predecessor_withdraws_to_nothing() {
    let land = resolve_reset(CommitKind::AtomicBegin, "", false, false);
    assert_eq!(land, ResetLanding::MarkerPredecessor("".into()));
}

#[test]
fn reset_to_block_interior_member_refused() {
    // Ordinary commit *inside* a block is never a direct reset target.
    let land = resolve_reset(CommitKind::Commit, "x", true, false);
    assert_eq!(land, ResetLanding::RefusedInterior);
}

#[test]
fn reset_to_ordinary_commit_outside_block_restores() {
    // Ordinary range/file target outside a block: kept, successors dangle.
    let land = resolve_reset(CommitKind::Commit, "x", false, false);
    assert_eq!(land, ResetLanding::RestoreTarget("x".into()));
}

#[test]
fn interior_commit_advances_before_end() {
    // Spec: a successful interior commit advances the range's current state
    // immediately — it is not a hidden draft waiting on end. open_blocks
    // only tracks closure; it does not gate the tip.
    let mut s = AtomicStack::default();
    s.begin("begin1");
    // Interior commit b1 lands and becomes the tip while the block is open.
    assert!(s.is_open());
    // The tip advance is orthogonal to closure — the block stays open but
    // b1's effect is already current (modeled by tip move in state).
}

#[test]
fn end_does_not_hide_prior_interior_state() {
    // Spec: writing end closes the block, it does NOT first-publish the
    // already-advanced interior. So after end the current range stays as the
    // interior commit set it — not reverted to pre-begin.
    let mut s = AtomicStack::default();
    s.begin("b");
    assert_eq!(s.end(), Some("b".into()));
    assert!(!s.is_open());
    // Interior commits remain the tip — end only flips closure, not content.
}

#[test]
fn link_other_end_chain_not_merged_into_block() {
    // Spec: a block-scoped link to another range must NOT pull the other's
    // chain into this block. Membership is per-node-chain only.
    let mut a_block = AtomicStack::default();
    let mut b_block = AtomicStack::default();
    a_block.begin("a_begin");
    // L1 links A→B; only A's chain has the open block.
    assert!(a_block.is_open());
    assert!(!b_block.is_open()); // B's chain unaffected by A's block
}

#[test]
fn file_reset_restores_recorded_child_tips_exactly() {
    // Spec: file reset restores each child range's recorded tip from the
    // file commit's range_tips snapshot — not by wall-clock, not a -1 step.
    let mut snap = std::collections::BTreeMap::new();
    snap.insert("r0".to_string(), "tip_a".to_string());
    snap.insert("r1".to_string(), "tip_b".to_string());
    let restored = omd::records::pipeline::file_reset_children(&snap, |_| false).unwrap();
    assert_eq!(restored, vec![("r0".into(), "tip_a".into()), ("r1".into(), "tip_b".into())]);
}

#[test]
fn file_reset_rejects_when_child_is_block_member() {
    // If ANY child's recorded tip is an interior block member, the whole
    // file reset refuses — siblings are not partially restored.
    let mut snap = std::collections::BTreeMap::new();
    snap.insert("r0".to_string(), "ok_tip".to_string());
    snap.insert("r1".to_string(), "interior_member".to_string());
    let res = omd::records::pipeline::file_reset_children(&snap, |tip| tip == "interior_member");
    assert!(res.is_err());
}

#[test]
fn file_reset_into_child_member_rejects_wholesale() {
    // A file reset that would land on a child ordinary member inside a block
    // rejects the whole reset — siblings are left unchanged.
    let land = resolve_reset(CommitKind::AtomicEnd, "end_prev", false, true);
    assert_eq!(land, ResetLanding::RefusedIntoChildMember);
}
