//! Tag resolution (E-7.2): flat project-local tags on nodes, inherited
//! recursively by directory members, additive + deduped. Same-named tags in
//! different projects never share identity — names are scoped to the store.

use std::collections::BTreeSet;

use crate::records::store::State;

/// Resolve the effective tag set for `node`: its own tags plus every tag on
/// ancestor mounts (a dir tag applies to members, including files added
/// later). Additive — a child's tag never replaces an inherited one; deduped
/// — the same tag inherited from multiple ancestors counts once.
///
/// `node` is a node key like `file:docs/a.md` or `dir:docs`. Inheritance
/// walks the mount tree upward from the node's parent path.
pub fn resolve_tags(state: &State, node: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    if let Some(t) = state.tags.get(node) {
        out.extend(t.iter().cloned());
    }
    // Inherit from ancestor path prefixes: `file:a/b/c.md` inherits tags on
    // `dir:a/b` and `dir:a`. Walk shortest→longest so nearer ancestors add
    // on top of (never replace) farther ones.
    if let Some(path) = node.strip_prefix("file:") {
        // Ancestor nodes are `file:` keys too (a dir is `file:docs`).
        let mut acc = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        for seg in &parts[..parts.len().saturating_sub(1)] {
            if !acc.is_empty() { acc.push('/'); }
            acc.push_str(seg);
            let dir_key = format!("file:{acc}");
            if let Some(t) = state.tags.get(&dir_key) {
                out.extend(t.iter().cloned());
            }
        }
    }
    out
}
