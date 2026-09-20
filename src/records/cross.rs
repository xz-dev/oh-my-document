//! Cross-store protection (E-11): peer registration, inbound credentials,
//! transitive protection closure, store activation.
//!
//! A store has an independent 128-bit `store_id` (never project_id or a
//! business commit id). Cross-store endpoints fix the target's store_id +
//! range identity + required version. Before a record on A references B's
//! target, B persists an *inbound credential* recording A's identity, the
//! pending record id, and the exact protected target — A publishes its
//! business record only after. The credential alone never proves the link;
//! A's live record does. B's gc must check A's effective record + the
//! transitive protection closure; when A is unreadable, B conservatively
//! protects the target (never releases on timeout/alias-loss/cache-miss).
//!
//! A copied store directory is a NEW writable authority only after explicit
//! re-registration (new store_id + completed external-reference
//! registration); until then it reads history for diagnostics only — no
//! business writes, no gc. A same-named dir/alias never silently takes over.

use crate::records::store::{InboundCred, PeerReg, State};

/// Generate a fresh 128-bit store_id.
pub fn new_store_id(rng: &dyn crate::testing::Rng) -> String {
    let mut b = [0u8; 16];
    rng.fill(&mut b);
    hex::encode(b)
}

/// Register a peer store (alias → locator). The locator is the local dir
/// the peer is bound to — never a frozen content snapshot.
pub fn register_peer(state: &mut State, store_id: &str, locator: &str) {
    state.peers.insert(
        store_id.to_string(),
        PeerReg { store_id: store_id.to_string(), locator: locator.to_string() },
    );
}

/// Persist an inbound protection credential: `peer_store_id` protecting our
/// `target` ahead of its `record_id` publishing. Returns the credential id.
pub fn persist_inbound(
    state: &mut State,
    peer_store_id: &str,
    record_id: &str,
    target: &str,
) -> String {
    let id = format!("in-{}-{}", peer_store_id, record_id);
    state.inbound.insert(
        id.clone(),
        InboundCred {
            peer_store_id: peer_store_id.to_string(),
            record_id: record_id.to_string(),
            target: target.to_string(),
        },
    );
    id
}

/// May this store run a business write or gc? An unregistered copy must not.
pub fn activated(state: &State) -> bool {
    state.activated
}

/// Activate a copied store: assign a NEW store_id, keep project_id and the
/// copied commit inputs/ids, and mark external-reference registration done.
/// Never recomputes project_id or copied commit ids, never repoints links.
pub fn activate(state: &mut State, rng: &dyn crate::testing::Rng) -> String {
    let id = new_store_id(rng);
    state.store_id = id.clone();
    state.activated = true;
    id
}
