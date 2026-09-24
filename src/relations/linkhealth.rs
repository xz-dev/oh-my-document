//! Link health classification and topological strata (link-query change).
//!
//! Five mutually exclusive states, judged per link with early exit at the
//! cheapest failing stratum:
//!
//! ```text
//! L0  structural    record readable + endpoint keys valid       (state only)
//! L1  liveness      pinned endpoint versions still effective    (state only)
//! L2  obligations   link_pending non-empty → obliged; persisted
//!                   dirty ranges on endpoint nodes → stale      (state only)
//! ```
//!
//! None of L0–L2 reads source content or executes a command. A command
//! endpoint that was not authorized to run this call is `unchecked` — a
//! count, never one of the five states: "cannot judge" is neither health
//! nor disease.
//!
//! Strata condense SCCs over the source→target digraph: a user-created
//! cycle is legal and its members share one stratum. Strata are a derived
//! read-only view recomputed per query, never persisted.

use std::collections::{BTreeMap, BTreeSet};

use crate::records::store::{Link, Store};

/// Five mutually exclusive health states, cheapest-judged-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkHealth {
    /// Record unreadable or an endpoint is not a valid object key.
    Broken,
    /// A pinned endpoint version was withdrawn (reset off / dangling).
    Withdrawn,
    /// Pending adapt obligations exist on the link.
    Obliged,
    /// Endpoints alive but persisted dirty state touches them.
    Stale,
    /// All strata pass; nothing pending, dirty, withdrawn, or broken.
    Healthy,
}

impl LinkHealth {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Broken => "broken",
            Self::Withdrawn => "withdrawn",
            Self::Obliged => "obliged",
            Self::Stale => "stale",
            Self::Healthy => "healthy",
        }
    }
}

/// Why a link landed in its state + which stratum first failed. Carried in
/// detail output; the summary only counts states.
#[derive(Debug, Clone)]
pub struct HealthDetail {
    pub health: LinkHealth,
    /// First failing stratum (0, 1, or 2); None for healthy.
    pub failing_stratum: Option<u8>,
    /// Human explanation of the first failure.
    pub reason: Option<String>,
}

/// Is `key` a structurally plausible local or peer object key?
///
/// `file:<hex>` / `range:<hex>` locally, `peer:<store-id>:<kind>:<hex>` for
/// cross-store endpoints. L0 asks only this — no content, no resolution.
fn plausible_endpoint(key: &str) -> bool {
    let valid_hex = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
    if let Some(rest) = key.strip_prefix("peer:") {
        let Some((_, remote)) = rest.split_once(':') else {
            return false;
        };
        let Some((kind, root)) = remote.split_once(':') else {
            return false;
        };
        (kind == "file" || kind == "range") && valid_hex(root)
    } else {
        match key.split_once(':') {
            Some(("file", root)) | Some(("range", root)) => valid_hex(root),
            // Journal endpoints (audit:/note:) are first-class link
            // endpoints — same chain-root id shape.
            Some(("audit", root)) | Some(("note", root)) => valid_hex(root),
            _ => false,
        }
    }
}

/// Judge one link's health with L0→L2 early exit.
///
/// `endpoint_alive` and `endpoint_dirty` keep this pure over state: the
/// caller supplies liveness (L1) and persisted-dirty (L2) lookups so the
/// classification itself never touches the filesystem.
pub fn judge_link(
    link: &Link,
    endpoint_alive: impl Fn(&str) -> bool,
    endpoint_dirty: impl Fn(&str) -> bool,
    pending_count: usize,
) -> HealthDetail {
    // L0 structural: both endpoints plausible object keys.
    if !plausible_endpoint(&link.source) || !plausible_endpoint(&link.target) {
        return HealthDetail {
            health: LinkHealth::Broken,
            failing_stratum: Some(0),
            reason: Some(format!(
                "endpoint key malformed: {} → {}",
                link.source, link.target
            )),
        };
    }
    // L1 liveness: pinned endpoint versions still on effective chains.
    if !endpoint_alive(&link.source) {
        return HealthDetail {
            health: LinkHealth::Withdrawn,
            failing_stratum: Some(1),
            reason: Some(format!("source endpoint withdrawn: {}", link.source)),
        };
    }
    if !endpoint_alive(&link.target) {
        return HealthDetail {
            health: LinkHealth::Withdrawn,
            failing_stratum: Some(1),
            reason: Some(format!("target endpoint withdrawn: {}", link.target)),
        };
    }
    // L2 obligations: pending adapt count wins over dirty (a concrete
    // to-do is more specific than "content drifted").
    if pending_count > 0 {
        return HealthDetail {
            health: LinkHealth::Obliged,
            failing_stratum: Some(2),
            reason: Some(format!("{pending_count} pending adapt obligation(s)")),
        };
    }
    if endpoint_dirty(&link.source) || endpoint_dirty(&link.target) {
        return HealthDetail {
            health: LinkHealth::Stale,
            failing_stratum: Some(2),
            reason: Some("endpoint carries persisted dirty ranges".into()),
        };
    }
    HealthDetail {
        health: LinkHealth::Healthy,
        failing_stratum: None,
        reason: None,
    }
}

/// L1 liveness for a local endpoint: the link's pinned `selected_version`
/// must still be reachable on the node's current effective chain. Peer
/// endpoints are judged alive — they are opaque ids by contract; their
/// protection credentials already guard publication.
pub fn endpoint_alive(store: &Store, endpoint: &str, pinned: &str) -> bool {
    if endpoint.starts_with("peer:") {
        return true;
    }
    // Walk the current tip back to genesis; the pinned commit must be on
    // the effective chain (reset moves the tip off withdrawn segments).
    let Some(mut current) = store.state().tips.get(endpoint).cloned() else {
        return false;
    };
    let mut guard = 0usize;
    while !current.is_empty() && guard < 100_000 {
        if current == pinned {
            return true;
        }
        let Ok(commit) = store.read_commit(&current) else {
            return false;
        };
        current = commit.previous_id;
        guard += 1;
    }
    false
}

/// L2 dirty predicate for an endpoint node: any persisted dirty ranges on
/// the node itself (range endpoint) or its child ranges (file endpoint).
pub fn endpoint_dirty(store: &Store, endpoint: &str) -> bool {
    let state = store.state();
    if let Some(dirty_state) = state.dirty.get(endpoint)
        && !dirty_state.dirty.is_empty()
    {
        return true;
    }
    // File endpoints: any mounted child range carrying dirty state.
    state
        .mounts
        .get(endpoint)
        .map(|children| {
            children.iter().any(|child| {
                state
                    .dirty
                    .get(child)
                    .is_some_and(|ds| !ds.dirty.is_empty())
            })
        })
        .unwrap_or(false)
}

/// SCC-condensed topological strata over the source→target digraph.
///
/// Stratum 0 holds roots (sources that are no link's target); a cycle is
/// legal and all its members share one stratum; anything strictly
/// downstream of stratum k gets a number > k. Peer endpoints participate
/// as ordinary opaque nodes. Returns node → stratum for every endpoint
/// mentioned by the given links.
///
/// Algorithm: Tarjan SCC (iterative, no recursion-depth hazard) then
/// longest-path layering over the condensed DAG.
pub fn link_strata(links: &[Link]) -> BTreeMap<String, u64> {
    // Collect nodes and adjacency.
    let mut nodes: BTreeSet<&str> = BTreeSet::new();
    for link in links {
        nodes.insert(link.source.as_str());
        nodes.insert(link.target.as_str());
    }
    let index_of = |key: &str| nodes.iter().position(|n| *n == key);
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for link in links {
        if let (Some(s), Some(t)) = (index_of(&link.source), index_of(&link.target)) {
            adjacency[s].push(t);
        }
    }

    // Tarjan SCC, iterative.
    let n = nodes.len();
    let mut scc_of: Vec<usize> = vec![usize::MAX; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut index: Vec<i64> = vec![-1; n];
    let mut low: Vec<i64> = vec![0; n];
    let mut next_index = 0i64;
    let mut scc_count = 0usize;

    for root in 0..n {
        if index[root] != -1 {
            continue;
        }
        // (node, child cursor)
        let mut call: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((v, cursor)) = call.pop() {
            if cursor == 0 {
                index[v] = next_index;
                low[v] = next_index;
                next_index += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            let mut recursed = false;
            for i in cursor..adjacency[v].len() {
                let w = adjacency[v][i];
                if index[w] == -1 {
                    // Recurse into w; resume v after its children.
                    call.push((v, i + 1));
                    call.push((w, 0));
                    recursed = true;
                    break;
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            }
            if recursed {
                continue;
            }
            // All children processed.
            if low[v] == index[v] {
                loop {
                    let w = stack.pop().expect("tarjan stack");
                    on_stack[w] = false;
                    scc_of[w] = scc_count;
                    if w == v {
                        break;
                    }
                }
                scc_count += 1;
            }
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[v]);
            }
        }
    }

    // Condensed DAG: edges scc→scc where different.
    let mut cond_adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); scc_count];
    for (v, targets) in adjacency.iter().enumerate() {
        for t in targets {
            if scc_of[v] != scc_of[*t] {
                cond_adj[scc_of[v]].insert(scc_of[*t]);
            }
        }
    }

    // Roots of the *condensed* graph: SCCs that are no edge's target.
    let mut is_target: Vec<bool> = vec![false; scc_count];
    for targets in &cond_adj {
        for t in targets {
            is_target[*t] = true;
        }
    }

    // Longest path from any root: stratum(scc) = 1 + max(stratum(preds)).
    // Iterative relaxation until fixed point (graph is a DAG after
    // condensation, so this terminates in ≤ scc_count rounds).
    let mut scc_stratum: Vec<i64> = vec![-1; scc_count];
    let mut changed = true;
    while changed {
        changed = false;
        for scc in 0..scc_count {
            if !is_target[scc] {
                if scc_stratum[scc] != 0 {
                    scc_stratum[scc] = 0;
                    changed = true;
                }
            } else {
                let mut best = -1i64;
                let mut reachable = false;
                for (from, targets) in cond_adj.iter().enumerate() {
                    if targets.contains(&scc) {
                        let f = scc_stratum[from];
                        if f >= 0 {
                            reachable = true;
                            best = best.max(f);
                        }
                    }
                }
                // Roots feed stratum 0; a non-root with all preds unassigned
                // would only occur in a cycle — impossible post-condensation.
                let value = if reachable { best + 1 } else { 0 };
                if scc_stratum[scc] != value {
                    scc_stratum[scc] = value;
                    changed = true;
                }
            }
        }
    }

    nodes
        .into_iter()
        .enumerate()
        .map(|(i, key)| (key.to_string(), scc_stratum[scc_of[i]].max(0) as u64))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(s: &str, t: &str) -> Link {
        Link {
            link_id: format!("L-{s}-{t}"),
            source: s.to_string(),
            target: t.to_string(),
            source_version: String::new(),
            target_version: String::new(),
            created_by: String::new(),
        }
    }

    fn judge(s: &str, t: &str, alive: bool, dirty: bool, pending: usize) -> HealthDetail {
        judge_link(&link(s, t), |_| alive, |_| dirty, pending)
    }

    #[test]
    fn malformed_endpoint_is_broken_at_l0() {
        let d = judge("not-a-key", "file:abc", true, false, 0);
        assert_eq!(d.health, LinkHealth::Broken);
        assert_eq!(d.failing_stratum, Some(0));
    }

    #[test]
    fn peer_key_shape_is_checked_at_l0() {
        let d = judge(
            "peer:store:file:deadbeef",
            "peer:store:bogus:x",
            true,
            false,
            0,
        );
        assert_eq!(d.health, LinkHealth::Broken, "non-hex root must fail L0");
        let d = judge("peer:store:range:01af", "file:01af", true, false, 0);
        assert_eq!(d.health, LinkHealth::Healthy);
    }

    #[test]
    fn dead_endpoint_is_withdrawn_at_l1() {
        let d = judge("file:ab", "file:cd", false, false, 0);
        assert_eq!(d.health, LinkHealth::Withdrawn);
        assert_eq!(d.failing_stratum, Some(1));
    }

    #[test]
    fn pending_beats_dirty_at_l2() {
        let d = judge("file:ab", "file:cd", true, true, 3);
        assert_eq!(d.health, LinkHealth::Obliged);
        assert_eq!(d.failing_stratum, Some(2));
        assert!(d.reason.unwrap().contains("3 pending"));
    }

    #[test]
    fn dirty_without_pending_is_stale() {
        let d = judge("file:ab", "file:cd", true, true, 0);
        assert_eq!(d.health, LinkHealth::Stale);
        assert_eq!(d.failing_stratum, Some(2));
    }

    #[test]
    fn clean_link_is_healthy() {
        let d = judge("file:ab", "file:cd", true, false, 0);
        assert_eq!(d.health, LinkHealth::Healthy);
        assert!(d.failing_stratum.is_none());
    }

    #[test]
    fn broken_never_calls_l1_or_l2() {
        // Endpoint predicates panic if consulted — proves L0 early exit.
        let d = judge_link(
            &link("garbage", "file:ab"),
            |_| panic!("L1 must not run after L0 failure"),
            |_| panic!("L2 must not run after L0 failure"),
            0,
        );
        assert_eq!(d.health, LinkHealth::Broken);
    }

    // ---- strata ----

    fn strata_of(specs: &[(&str, &str)]) -> BTreeMap<String, u64> {
        let links: Vec<Link> = specs.iter().map(|(s, t)| link(s, t)).collect();
        link_strata(&links)
    }

    #[test]
    fn dag_strata_increase_downstream() {
        let s = strata_of(&[("a", "b"), ("b", "c"), ("a", "c")]);
        assert_eq!(s["a"], 0);
        assert_eq!(s["b"], 1);
        assert_eq!(s["c"], 2, "c sits after both a and b — longest path wins");
    }

    #[test]
    fn cycle_members_share_a_stratum() {
        let s = strata_of(&[("a", "b"), ("b", "c"), ("c", "a")]);
        assert_eq!(s["a"], s["b"]);
        assert_eq!(s["b"], s["c"]);
    }

    #[test]
    fn downstream_of_cycle_is_strictly_later() {
        let s = strata_of(&[("a", "b"), ("b", "c"), ("c", "a"), ("c", "d")]);
        let cycle = s["a"];
        assert_eq!(s["b"], cycle);
        assert_eq!(s["c"], cycle);
        assert!(s["d"] > cycle, "d must be strictly downstream");
    }

    #[test]
    fn self_loop_is_legal_and_rooted() {
        let s = strata_of(&[("a", "a")]);
        assert_eq!(s["a"], 0, "self-loop SCC containing a root stays stratum 0");
    }

    #[test]
    fn peer_endpoints_are_ordinary_nodes() {
        let s = strata_of(&[("a", "peer:s:file:01"), ("peer:s:file:01", "b")]);
        assert_eq!(s["a"], 0);
        assert_eq!(s["peer:s:file:01"], 1);
        assert_eq!(s["b"], 2);
    }

    #[test]
    fn empty_link_set_yields_empty_strata() {
        assert!(link_strata(&[]).is_empty());
    }
}
