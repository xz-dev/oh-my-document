//! Group 6/8 domain semantics: obligation stacking, immediate dangling
//! failure, link identity, adapt preconditions.

use std::collections::{BTreeMap, BTreeSet};

use omd::records::ids::Id128;
use omd::records::ops::Link;
use omd::relations::dirty::DirtyState;

#[test]
fn unclean_obligations_stack_not_merge() {
    // Multiple `unclean` on the same content keep distinct ids/reasons —
    // each is its own obligation, never collapsed onto one entry.
    let mut s = DirtyState::default();
    s.push_unclean("u1", "first");
    s.push_unclean("u2", "second");
    assert_eq!(s.obligations.len(), 2);
    assert_eq!(s.obligations[0].commit_id, "u1");
    assert_eq!(s.obligations[1].commit_id, "u2");
    assert_eq!(s.obligations[1].atop, "u1"); // stacked on prior
}

#[test]
fn dangling_dependency_fails_at_first_broken_hop() {
    // c1 -> b1 -> a1 ; when a1 is dangling, resolution fails AT a1 —
    // immediately, not lazily at read time.
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    edges.insert("c1".into(), ["b1"].into_iter().map(String::from).collect());
    edges.insert("b1".into(), ["a1"].into_iter().map(String::from).collect());
    let present = |id: &str| id != "a1"; // a1 dangling
    let mut s = DirtyState::default();
    let err = s.propagate(&edges, &present).unwrap_err();
    assert_eq!(err.missing, "a1");
    // The dependent that relied on the missing hop is marked.
    assert!(s.is_dirty("b1"));
}

// `history_or_clean_cannot_repair_dangling` removed — it marked then
// `is_dirty` without attempting repair (vacuous). Dangling/non-repair
// coverage is real in tests/reset.rs + tests/traceability.rs
// `dangling_commit_still_inspectable`.

#[test]
fn link_instances_with_same_endpoints_are_distinct() {
    // Two links between identical source/target are separate instances via
    // distinct link_id — adaptation addresses one, not both.
    let l1 = Link {
        link_id: Id128([1; 16]),
        source: "a@0-5:cA".into(),
        target: "b@0-5:cB".into(),
        reason: "r".into(),
    };
    let l2 = Link {
        link_id: Id128([2; 16]),
        source: "a@0-5:cA".into(),
        target: "b@0-5:cB".into(),
        reason: "r".into(),
    };
    assert_ne!(l1.link_id, l2.link_id);
    assert_eq!(l1.source, l2.source);
}

#[test]
fn cycles_terminate_without_repeat_obligations() {
    // A→B→C→A: reachability terminates (spec: 不因成环无限执行) and never
    // re-obligates the same node. Distinct nodes stay distinct.
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    edges.insert("A".into(), ["B".to_string()].into_iter().collect());
    edges.insert("B".into(), ["C".to_string()].into_iter().collect());
    edges.insert("C".into(), ["A".to_string()].into_iter().collect());
    let order = omd::relations::dirty::DirtyState::reachable_bounded("A", &edges);
    // Each visited once — no A re-entry.
    assert_eq!(order.len(), 3);
    assert!(
        order.contains(&"A".to_string())
            && order.contains(&"B".to_string())
            && order.contains(&"C".to_string())
    );
}

// `adapt_requires_link_id_changes_reason` removed — asserting struct fields
// non-empty is a tautology. Real adapt-rejection coverage in tests/guards.rs:
// adapt_without_link_id_rejected / adapt_without_reason_rejected /
// adapt_no_reason_rejected.
