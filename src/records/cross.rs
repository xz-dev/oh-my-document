//! Cross-store protection: immutable logical peer/inbound records plus
//! machine-local placement. Business publication remains one authority; a
//! receipt only protects its exact target until the consumer can be checked.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::records::store::{InboundCred, PeerExpected, PeerReg, Store, StoreError};

pub fn new_store_id(rng: &dyn crate::testing::Rng) -> String {
    let mut b = [0u8; 16];
    rng.fill(&mut b);
    hex::encode(b)
}

fn record_id<T: serde::Serialize>(record: &T) -> Result<String, StoreError> {
    let bytes = toml::to_string(record).map_err(|error| StoreError::Record(error.to_string()))?;
    Ok(hex::encode(Sha256::digest(bytes.as_bytes())))
}

pub fn read_peer(store: &Store, id: &str) -> Result<PeerReg, StoreError> {
    let text = std::fs::read_to_string(store.root().join(format!("registrations/{id}.toml")))?;
    let record: PeerReg =
        toml::from_str(&text).map_err(|error| StoreError::Record(error.to_string()))?;
    if record.format != "omd.peer-registration/1" || record_id(&record)? != id {
        return Err(StoreError::Record(format!(
            "peer registration {id} has unsupported format or inconsistent identity"
        )));
    }
    Ok(record)
}

pub fn read_inbound(store: &Store, id: &str) -> Result<InboundCred, StoreError> {
    let text = std::fs::read_to_string(store.root().join(format!("inbound/{id}.toml")))?;
    let record: InboundCred =
        toml::from_str(&text).map_err(|error| StoreError::Record(error.to_string()))?;
    if record.format != "omd.inbound/2"
        || record.credential_id != id
        || record_id(&InboundCred {
            credential_id: String::new(),
            ..record.clone()
        })? != id
        || record.record_id.is_empty()
        || record.link_id.is_empty()
        || !crate::relations::node::is_range_key(&record.target_root)
        || crate::relations::identity::resolve_version_on_chain(
            store,
            &record.target_root,
            &record.target_version,
        )
        .is_err()
    {
        return Err(StoreError::Record(format!(
            "inbound credential {id} has unsupported format or inconsistent identity"
        )));
    }
    Ok(record)
}

/// Publish one immutable logical peer-registration revision. Physical paths
/// are intentionally absent; callers persist those in machine-local config.
pub fn register_peer(
    store: &mut Store,
    peer_project_id: &str,
    peer_store_id: &str,
) -> Result<String, StoreError> {
    store.lock()?;
    if peer_project_id.len() != 32 || peer_store_id.len() != 32 {
        return Err(StoreError::Record(
            "peer project/store identities must be 32 lowercase hexadecimal characters".into(),
        ));
    }
    let previous_id = store.state().peers.get(peer_store_id).cloned();
    let previous = previous_id
        .as_deref()
        .map(|id| read_peer(store, id))
        .transpose()?;
    if previous
        .as_ref()
        .is_some_and(|record| record.peer_project_id == peer_project_id)
    {
        return Ok(previous_id.unwrap());
    }
    let record = PeerReg {
        format: "omd.peer-registration/1".into(),
        peer_project_id: peer_project_id.into(),
        peer_store_id: peer_store_id.into(),
        revision: previous.as_ref().map_or(1, |record| record.revision + 1),
        previous_id,
    };
    let id = record_id(&record)?;
    let bytes = toml::to_string(&record).map_err(|error| StoreError::Record(error.to_string()))?;
    store.write_record(&format!("registrations/{id}.toml"), bytes.as_bytes())?;
    let mut state = store.state().clone();
    state.peers.insert(peer_store_id.into(), id.clone());
    state.publication += 1;
    store.set_state_locked_public(state)?;
    Ok(id)
}

/// Persist exact protection before consumer publication. Repeating identical
/// input reuses the immutable record; changing any field creates another ID.
pub fn persist_inbound(
    store: &mut Store,
    consumer_project_id: &str,
    consumer_store_id: &str,
    consumer_registration_id: &str,
    record_id_value: &str,
    link_id: &str,
    target_root: &str,
    target_version: &str,
) -> Result<String, StoreError> {
    store.lock()?;
    store.require_inbound_authority()?;
    if store
        .state()
        .peers
        .get(consumer_store_id)
        .map(String::as_str)
        != Some(consumer_registration_id)
    {
        return Err(StoreError::Conflict(format!(
            "consumer registration is not selected: {consumer_store_id}"
        )));
    }
    if !crate::relations::node::is_range_key(target_root) {
        return Err(StoreError::Record(
            "inbound protection target must be a range chain root".into(),
        ));
    }
    crate::relations::identity::resolve_version_on_chain(store, target_root, target_version)
        .map_err(|error| StoreError::Record(format!("inbound target version: {error}")))?;
    let mut record = InboundCred {
        format: "omd.inbound/2".into(),
        credential_id: String::new(),
        consumer_project_id: consumer_project_id.into(),
        consumer_store_id: consumer_store_id.into(),
        consumer_registration_id: consumer_registration_id.into(),
        record_id: record_id_value.into(),
        link_id: link_id.into(),
        target_root: target_root.into(),
        target_version: target_version.into(),
    };
    let id = record_id(&record)?;
    record.credential_id = id.clone();
    let bytes = toml::to_string(&record).map_err(|error| StoreError::Record(error.to_string()))?;
    store.write_record(&format!("inbound/{id}.toml"), bytes.as_bytes())?;
    let mut state = store.state().clone();
    state.inbound.insert(id.clone(), id.clone());
    state.publication += 1;
    store.set_state_locked_public(state)?;
    Ok(id)
}

pub fn copied_project_registrations(
    store: &Store,
    new_store_id: &str,
) -> Result<BTreeMap<String, String>, StoreError> {
    let mut selected = store.state().registrations.clone();
    for (key, current_id) in &store.state().registrations {
        let Some(alias) = key.strip_prefix("project:") else {
            continue;
        };
        let current = store.project_registration(alias)?;
        let record = crate::records::registration::ProjectRegistration::new(
            alias.to_string(),
            current.project_id,
            new_store_id.to_string(),
            current.revision + 1,
            current.remote,
        );
        let id = record
            .id()
            .map_err(|error| StoreError::Record(error.to_string()))?;
        if &id == current_id {
            return Err(StoreError::Record(
                "copy registration revision did not change identity".into(),
            ));
        }
        let bytes =
            toml::to_string(&record).map_err(|error| StoreError::Record(error.to_string()))?;
        store.write_record(&format!("registrations/{id}.toml"), bytes.as_bytes())?;
        selected.insert(key.clone(), id);
    }
    Ok(selected)
}

/// Revalidate a peer's logical registration, physical placement, mapping
/// revision, and selected publication after canonical store locks are held.
/// On success this Store value receives a narrow inbound-protection grant for
/// the exact selected participant; ordinary business writes remain forbidden.
pub fn validate_peer_locked(
    owner: &Store,
    peer: &mut Store,
    peer_store_id: &str,
    expected: &PeerExpected,
) -> Result<(), StoreError> {
    if !owner.is_locked() || !peer.is_locked() {
        return Err(StoreError::Conflict(
            "peer validation requires all participants to be locked".into(),
        ));
    }
    let cwd = owner.config_cwd().ok_or_else(|| {
        StoreError::Conflict("peer validation requires machine-local configuration".into())
    })?;
    let registration_id = owner
        .state()
        .peers
        .get(peer_store_id)
        .ok_or_else(|| StoreError::Conflict(format!("peer not registered: {peer_store_id}")))?;
    let registration = read_peer(owner, registration_id)?;
    let mapping =
        crate::sources::projects::peer_mapping(cwd, &owner.state().store_id, peer_store_id)
            .map_err(|error| StoreError::Conflict(error.to_string()))?;
    let mapped_metadata = std::fs::canonicalize(&mapping.metadata_root)?;
    let selected_metadata = std::fs::canonicalize(peer.root())?;
    if mapped_metadata != selected_metadata {
        return Err(StoreError::Conflict(format!(
            "peer physical metadata instance changed: {peer_store_id}"
        )));
    }
    let identity = peer.identity();
    if mapping.peer_registration_id != *registration_id
        || mapping.peer_store_id != peer_store_id
        || mapping.peer_project_id != identity.project_id
        || registration.peer_store_id != peer_store_id
        || registration.peer_project_id != identity.project_id
        || identity.store_id != peer_store_id
    {
        return Err(StoreError::Conflict(format!(
            "peer identity or registration changed: {peer_store_id}"
        )));
    }
    let instance = crate::sources::projects::context_instance(
        &mapping.project_root,
        &mapping.metadata_root,
        &identity,
    )
    .map_err(|error| StoreError::Conflict(error.to_string()))?;
    if expected.instance != instance
        || expected.mapping_revision != mapping.revision
        || expected.project_id != identity.project_id
        || expected.store_id != identity.store_id
        || expected.publication != peer.state().publication
        || expected.tips != peer.state().tips
        || expected.registrations != peer.state().registrations
        || expected.peers != peer.state().peers
    {
        return Err(StoreError::Conflict(format!(
            "peer observation is stale or incomplete: {peer_store_id}"
        )));
    }
    peer.bind_context(
        instance,
        Some(mapping.revision),
        mapping.project_root,
        None,
        cwd.to_path_buf(),
        Default::default(),
    );
    peer.mark_validated_peer_instance();
    Ok(())
}

/// Local authority is not a copied shared bit. A store is writable only when
/// selected through one exact machine-local project mapping to this instance.
pub fn activated(store: &Store) -> bool {
    store.has_write_authority()
}
