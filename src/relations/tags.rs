//! Tag resolution: flat project-local tags persisted on object references.
//! Location inheritance consults `State::locations`; paths are never decoded
//! from object keys.

use std::collections::BTreeSet;

use crate::records::store::State;

pub fn resolve_tags(state: &State, node: &str) -> BTreeSet<String> {
    let mut out = state.tags.get(node).cloned().unwrap_or_default();
    let Some(path) = state.locations.get(node) else {
        return out;
    };
    for (ancestor, tags) in &state.tags {
        let Some(parent_path) = state.locations.get(ancestor) else {
            continue;
        };
        let prefix = format!("{}/", parent_path.trim_end_matches('/'));
        if path.starts_with(&prefix) {
            out.extend(tags.iter().cloned());
        }
    }
    out
}
