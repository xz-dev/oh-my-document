//! Shared project/store aliases and machine-local placement selection.
//!
//! Immutable registration records selected by `state.toml` hold shared
//! alias/remote authority. `<config-root>/projects.toml` stores only this
//! machine's placement and exact-revision local remote approvals.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::records::registration::RemoteIdentity;
use crate::records::store::{Expected, Store, StoreError, StoreIdentity};
use crate::sources::discovery::config_dir;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("project configuration unreadable: {0}")]
    Io(#[from] std::io::Error),
    #[error("project configuration invalid: {0}")]
    Invalid(String),
    #[error("unsupported project configuration format '{0}'")]
    Unsupported(String),
    #[error("project alias is not registered: {0}")]
    MissingAlias(String),
    #[error("project selection is ambiguous: {0}")]
    Ambiguous(String),
    #[error("project location is invalid: {0}")]
    Location(String),
    #[error("project/store identity mismatch: {0}")]
    Identity(String),
    #[error("project evidence conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

fn mutation_error(error: StoreError) -> ProjectError {
    match error {
        StoreError::Conflict(message) => ProjectError::Conflict(message),
        other => ProjectError::Store(other),
    }
}

/// One machine-local placement of an existing logical project/store pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMap {
    pub alias: String,
    pub project_id: String,
    pub store_id: String,
    pub project_root: PathBuf,
    pub metadata_root: PathBuf,
    /// Explicit raw remote URL values approved for this exact logical
    /// registration and physical checkout instance.
    #[serde(default)]
    pub recognized_remote_urls: BTreeSet<String>,
    /// Shared registration revision these approvals were made against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognized_registration: Option<String>,
    /// Monotonic machine-local placement/approval revision.
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerMap {
    pub owner_store_id: String,
    pub peer_project_id: String,
    pub peer_store_id: String,
    pub project_root: PathBuf,
    pub metadata_root: PathBuf,
    pub peer_registration_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityMap {
    pub project_id: String,
    pub store_id: String,
    pub project_root: PathBuf,
    pub metadata_root: PathBuf,
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Maps {
    format: String,
    #[serde(default)]
    project: Vec<ProjectMap>,
    #[serde(default)]
    peer: Vec<PeerMap>,
    #[serde(default)]
    authority: Vec<AuthorityMap>,
}

#[derive(Debug, Clone)]
pub enum MetadataSelection {
    Existing(StoreIdentity),
    Initialize,
    InitializeConfigured,
}

/// Single selected project/store context consumed by file observation and
/// metadata callers. Paths are machine-local; identity is stable/shared.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub project_root: PathBuf,
    pub metadata_root: PathBuf,
    pub metadata: MetadataSelection,
    /// Physical metadata instance fingerprint. Machine-local only; never a
    /// business hash input.
    pub instance: String,
    pub mapping_revision: Option<u64>,
    pub alias: Option<String>,
    pub recognized_remote_urls: BTreeSet<String>,
}

pub fn context_instance(
    project_root: &Path,
    metadata_root: &Path,
    identity: &StoreIdentity,
) -> Result<String, ProjectError> {
    let project = fs::canonicalize(project_root)?;
    let metadata = fs::canonicalize(metadata_root)?;
    let mut hash = Sha256::new();
    for value in [
        project.as_os_str().as_encoded_bytes(),
        metadata.as_os_str().as_encoded_bytes(),
        identity.project_id.as_bytes(),
        identity.store_id.as_bytes(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    Ok(hex::encode(hash.finalize()))
}

fn maps_path(cwd: &Path) -> PathBuf {
    config_dir(cwd).join("projects.toml")
}

/// Fail-closed load. Only an absent file means no mappings.
fn load_maps(cwd: &Path) -> Result<Maps, ProjectError> {
    let path = maps_path(cwd);
    if !path.exists() {
        return Ok(Maps {
            format: "omd.projects/4".into(),
            project: Vec::new(),
            peer: Vec::new(),
            authority: Vec::new(),
        });
    }
    let text = fs::read_to_string(&path)?;
    let maps: Maps = toml::from_str(&text).map_err(|e| ProjectError::Invalid(e.to_string()))?;
    if maps.format != "omd.projects/4" {
        return Err(ProjectError::Unsupported(maps.format));
    }
    for map in &maps.project {
        if map.alias.is_empty()
            || map.project_id.is_empty()
            || map.store_id.is_empty()
            || map.revision == 0
            || map
                .recognized_remote_urls
                .iter()
                .any(|url| url.is_empty() || url.contains(['\n', '\r', '\0']))
            || map
                .recognized_registration
                .as_ref()
                .is_some_and(String::is_empty)
            || (!map.recognized_remote_urls.is_empty() && map.recognized_registration.is_none())
            || !map.project_root.is_absolute()
            || !map.metadata_root.is_absolute()
        {
            return Err(ProjectError::Invalid(
                "mapping requires alias, identities, and absolute roots".into(),
            ));
        }
    }
    for peer in &maps.peer {
        if peer.owner_store_id.is_empty()
            || peer.peer_project_id.is_empty()
            || peer.peer_store_id.is_empty()
            || peer.peer_registration_id.is_empty()
            || peer.revision == 0
            || !peer.project_root.is_absolute()
            || !peer.metadata_root.is_absolute()
        {
            return Err(ProjectError::Invalid(
                "peer mapping requires identities, registration, and absolute roots".into(),
            ));
        }
    }
    for authority in &maps.authority {
        if authority.project_id.is_empty()
            || authority.store_id.is_empty()
            || authority.revision == 0
            || !authority.project_root.is_absolute()
            || !authority.metadata_root.is_absolute()
        {
            return Err(ProjectError::Invalid(
                "authority mapping requires identities and absolute roots".into(),
            ));
        }
    }
    Ok(maps)
}

pub fn load(cwd: &Path) -> Result<Vec<ProjectMap>, ProjectError> {
    Ok(load_maps(cwd)?.project)
}

pub fn load_peers(cwd: &Path) -> Result<Vec<PeerMap>, ProjectError> {
    Ok(load_maps(cwd)?.peer)
}

pub fn peer_mapping(
    cwd: &Path,
    owner_store_id: &str,
    peer_store_id: &str,
) -> Result<PeerMap, ProjectError> {
    let matching: Vec<_> = load_peers(cwd)?
        .into_iter()
        .filter(|map| map.owner_store_id == owner_store_id && map.peer_store_id == peer_store_id)
        .collect();
    match matching.as_slice() {
        [map] => Ok(map.clone()),
        [] => Err(ProjectError::MissingAlias(format!(
            "peer store {peer_store_id} for {owner_store_id}"
        ))),
        _ => Err(ProjectError::Ambiguous(format!(
            "peer store {peer_store_id} for {owner_store_id}"
        ))),
    }
}

pub fn save_peer_mapping(cwd: &Path, mapping: PeerMap) -> Result<PeerMap, ProjectError> {
    let mut maps = load_maps(cwd)?;
    maps.peer.retain(|map| {
        !(map.owner_store_id == mapping.owner_store_id
            && map.peer_store_id == mapping.peer_store_id)
    });
    maps.peer.push(mapping.clone());
    write_maps(cwd, &maps.project, &maps.peer, &maps.authority)?;
    Ok(mapping)
}

pub fn authorize_instance(
    cwd: &Path,
    project_root: &Path,
    metadata_root: &Path,
    identity: &StoreIdentity,
) -> Result<AuthorityMap, ProjectError> {
    let project_root = fs::canonicalize(project_root)?;
    let metadata_root = fs::canonicalize(metadata_root)?;
    let mut maps = load_maps(cwd)?;
    let revision = maps
        .authority
        .iter()
        .filter(|map| map.store_id == identity.store_id)
        .map(|map| map.revision)
        .max()
        .unwrap_or(0)
        + 1;
    maps.authority.retain(|map| {
        !(map.project_id == identity.project_id && map.store_id == identity.store_id)
    });
    let authority = AuthorityMap {
        project_id: identity.project_id.clone(),
        store_id: identity.store_id.clone(),
        project_root,
        metadata_root,
        revision,
    };
    maps.authority.push(authority.clone());
    write_maps(cwd, &maps.project, &maps.peer, &maps.authority)?;
    Ok(authority)
}

pub fn authority_matches(
    cwd: &Path,
    project_root: &Path,
    metadata_root: &Path,
    identity: &StoreIdentity,
) -> Result<bool, ProjectError> {
    let project_root = fs::canonicalize(project_root)?;
    let metadata_root = fs::canonicalize(metadata_root)?;
    let maps = load_maps(cwd)?;
    let matching: Vec<_> = maps
        .authority
        .iter()
        .filter(|map| {
            map.project_id == identity.project_id
                && map.store_id == identity.store_id
                && map.project_root == project_root
                && map.metadata_root == metadata_root
        })
        .collect();
    Ok(matches!(matching.as_slice(), [_]))
}

pub fn retarget_store_instance(
    cwd: &Path,
    metadata_root: &Path,
    project_id: &str,
    old_store_id: &str,
    new_store_id: &str,
) -> Result<(), ProjectError> {
    let metadata_root = fs::canonicalize(metadata_root)?;
    let mut maps = load_maps(cwd)?;
    for project in &mut maps.project {
        if project.project_id == project_id
            && project.store_id == old_store_id
            && project.metadata_root == metadata_root
        {
            project.store_id = new_store_id.to_string();
            project.revision += 1;
            project.recognized_remote_urls.clear();
            project.recognized_registration = None;
        }
    }
    write_maps(cwd, &maps.project, &maps.peer, &maps.authority)
}

fn write_maps(
    cwd: &Path,
    projects: &[ProjectMap],
    peers: &[PeerMap],
    authorities: &[AuthorityMap],
) -> Result<(), ProjectError> {
    let path = maps_path(cwd);
    let text = toml::to_string(&Maps {
        format: "omd.projects/4".into(),
        project: projects.to_vec(),
        peer: peers.to_vec(),
        authority: authorities.to_vec(),
    })
    .map_err(|e| ProjectError::Invalid(e.to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Register shared alias authority plus this machine's placement. Validation
/// completes before either authority is changed. This never activates a copy.
pub fn register(
    cwd: &Path,
    alias: &str,
    project_root: PathBuf,
    metadata_root: PathBuf,
    remote: Option<RemoteIdentity>,
    expected: &Expected,
) -> Result<ProjectMap, ProjectError> {
    if alias.is_empty() || alias == "root" {
        return Err(ProjectError::Invalid(
            "alias must be non-empty and cannot be reserved alias 'root'".into(),
        ));
    }
    let project_root = absolute_lexical(cwd, &project_root)?;
    let metadata_root = absolute_lexical(cwd, &metadata_root)?;
    if !project_root.is_dir() {
        return Err(ProjectError::Location(format!(
            "project root does not exist: {}",
            project_root.display()
        )));
    }
    let identity =
        Store::identity_at(&metadata_root).map_err(|e| ProjectError::Location(e.to_string()))?;
    let mut maps = load(cwd)?;
    let observed_maps = maps.clone();
    if maps.iter().any(|map| {
        map.alias == alias
            && (map.project_id != identity.project_id || map.store_id != identity.store_id)
            && !(map.project_id == identity.project_id && map.metadata_root == metadata_root)
    }) {
        return Err(ProjectError::Identity(format!(
            "alias {alias} already binds a different project/store"
        )));
    }
    if maps.iter().any(|map| {
        map.store_id == identity.store_id
            && map.metadata_root != metadata_root
            && map.metadata_root.exists()
    }) {
        return Err(ProjectError::Identity(format!(
            "store {} already has another live local metadata placement",
            identity.store_id
        )));
    }

    let previous = maps
        .iter()
        .find(|map| map.alias == alias && map.store_id == identity.store_id)
        .cloned();
    let evidence_map = previous.clone().or_else(|| {
        maps.iter()
            .find(|map| {
                map.project_id == identity.project_id
                    && map.store_id == identity.store_id
                    && map.project_root == project_root
                    && map.metadata_root == metadata_root
            })
            .cloned()
    });
    let revision = maps
        .iter()
        .filter(|map| map.alias == alias && map.store_id == identity.store_id)
        .map(|map| map.revision)
        .max()
        .unwrap_or(0)
        + 1;
    let preserve_recognition = previous
        .as_ref()
        .is_some_and(|map| map.project_root == project_root && map.metadata_root == metadata_root);
    let mut new_map = ProjectMap {
        alias: alias.to_string(),
        project_id: identity.project_id.clone(),
        store_id: identity.store_id.clone(),
        project_root,
        metadata_root: metadata_root.clone(),
        recognized_remote_urls: previous
            .as_ref()
            .filter(|_| preserve_recognition)
            .map(|map| map.recognized_remote_urls.clone())
            .unwrap_or_default(),
        recognized_registration: previous
            .as_ref()
            .filter(|_| preserve_recognition)
            .and_then(|map| map.recognized_registration.clone()),
        revision,
    };
    maps.retain(|map| !(map.alias == alias && map.store_id == identity.store_id));

    let mut store =
        Store::open_existing(&metadata_root).map_err(|e| ProjectError::Location(e.to_string()))?;
    store.bind_context(
        context_instance(&new_map.project_root, &metadata_root, &identity)?,
        evidence_map.as_ref().map(|map| map.revision),
        new_map.project_root.clone(),
        evidence_map.as_ref().map(|map| map.alias.clone()),
        cwd.to_path_buf(),
        evidence_map
            .as_ref()
            .map(|map| map.recognized_remote_urls.clone())
            .unwrap_or_default(),
    );
    store.lock().map_err(mutation_error)?;
    store
        .check_metadata_expected(expected)
        .map_err(mutation_error)?;
    if load(cwd)? != observed_maps {
        return Err(ProjectError::Conflict(format!(
            "project mapping {alias} changed before lock acquisition"
        )));
    }
    let registration_id = store
        .register_project(alias, remote, expected)
        .map_err(mutation_error)?;
    if new_map.recognized_registration.as_deref() != Some(&registration_id) {
        new_map.recognized_remote_urls.clear();
        new_map.recognized_registration = None;
    }
    maps.push(new_map.clone());
    let current = load_maps(cwd)?;
    write_maps(cwd, &maps, &current.peer, &current.authority)?;
    let prior_authorities: Vec<_> = current
        .authority
        .iter()
        .filter(|authority| {
            authority.project_id == identity.project_id && authority.store_id == identity.store_id
        })
        .collect();
    if let [authority] = prior_authorities.as_slice()
        && authority.metadata_root != new_map.metadata_root
        && !authority.metadata_root.exists()
    {
        authorize_instance(
            cwd,
            &new_map.project_root,
            &new_map.metadata_root,
            &identity,
        )?;
    }
    Ok(new_map)
}

pub fn recognize_remote(
    cwd: &Path,
    alias: &str,
    url: &str,
    expected: &Expected,
) -> Result<ProjectMap, ProjectError> {
    if url.is_empty() || url.contains(['\n', '\r', '\0']) {
        return Err(ProjectError::Invalid(
            "recognized remote URL must be a non-empty single-line value".into(),
        ));
    }
    let mut maps = load(cwd)?;
    let matching: Vec<_> = maps
        .iter()
        .enumerate()
        .filter(|(_, map)| map.alias == alias)
        .map(|(index, _)| index)
        .collect();
    let [index] = matching.as_slice() else {
        return Err(if matching.is_empty() {
            ProjectError::MissingAlias(alias.into())
        } else {
            ProjectError::Ambiguous(format!("alias {alias}"))
        });
    };
    let current = maps[*index].clone();
    let identity = Store::identity_at(&current.metadata_root)
        .map_err(|error| ProjectError::Location(error.to_string()))?;
    let mut store = Store::open_existing(&current.metadata_root)
        .map_err(|error| ProjectError::Location(error.to_string()))?;
    store.bind_context(
        context_instance(&current.project_root, &current.metadata_root, &identity)?,
        Some(current.revision),
        current.project_root.clone(),
        Some(current.alias.clone()),
        cwd.to_path_buf(),
        current.recognized_remote_urls.clone(),
    );
    store.lock().map_err(mutation_error)?;
    store
        .check_metadata_expected(expected)
        .map_err(mutation_error)?;
    let fresh_maps = load(cwd)?;
    let fresh = fresh_maps
        .iter()
        .find(|map| {
            map.alias == current.alias
                && map.store_id == current.store_id
                && map.project_root == current.project_root
                && map.metadata_root == current.metadata_root
        })
        .ok_or_else(|| {
            ProjectError::Conflict(format!(
                "project mapping {alias} changed before lock acquisition"
            ))
        })?;
    if fresh.revision != current.revision
        || fresh.recognized_remote_urls != current.recognized_remote_urls
        || fresh.recognized_registration != current.recognized_registration
    {
        return Err(ProjectError::Conflict(format!(
            "project mapping {alias} changed before lock acquisition"
        )));
    }
    let registration = store
        .project_registration(alias)
        .map_err(|error| ProjectError::Identity(error.to_string()))?;
    if registration.remote.is_none() {
        return Err(ProjectError::Invalid(format!(
            "project alias {alias} has no remote identity constraint"
        )));
    }
    let next_revision = maps
        .iter()
        .filter(|map| map.alias == alias && map.store_id == current.store_id)
        .map(|map| map.revision)
        .max()
        .unwrap_or(0)
        + 1;
    maps[*index].recognized_remote_urls.insert(url.to_string());
    maps[*index].recognized_registration = Some(
        store
            .identity()
            .registrations
            .get(&format!("project:{alias}"))
            .cloned()
            .ok_or_else(|| {
                ProjectError::Identity(format!(
                    "project alias {alias} has no selected registration"
                ))
            })?,
    );
    maps[*index].revision = next_revision;
    let updated = maps[*index].clone();
    let all = load_maps(cwd)?;
    write_maps(cwd, &maps, &all.peer, &all.authority)?;
    Ok(updated)
}

/// Resolve and validate a mapped project-relative source. `root` means the
/// selected owning context. Shared descriptors persist only `logical`.
pub fn resolve_path(
    cwd: &Path,
    current_root: &Path,
    alias: &str,
    path: &str,
) -> Result<(String, PathBuf), ProjectError> {
    let root = if alias == "root" {
        current_root.to_path_buf()
    } else {
        let map = select_alias(&load(cwd)?, alias, None, cwd)?;
        validate_source_map(cwd, &map)?;
        map.project_root
    };
    project_path(&root, path)
}

/// Resolve a mapped project-relative source to its machine-local path.
pub fn resolve(
    cwd: &Path,
    current_root: &Path,
    alias: &str,
    path: &str,
) -> Result<PathBuf, ProjectError> {
    resolve_path(cwd, current_root, alias, path).map(|(_, absolute)| absolute)
}

fn validate_source_map(cwd: &Path, map: &ProjectMap) -> Result<(), ProjectError> {
    let identity = Store::identity_at(&map.metadata_root)
        .map_err(|error| ProjectError::Identity(error.to_string()))?;
    if identity.project_id != map.project_id || identity.store_id != map.store_id {
        return Err(ProjectError::Identity(format!(
            "mapping {} expects project {} store {}, found project {} store {}",
            map.alias, map.project_id, map.store_id, identity.project_id, identity.store_id
        )));
    }
    let mut store = Store::open_existing(&map.metadata_root)
        .map_err(|error| ProjectError::Identity(error.to_string()))?;
    let registration = store.project_registration(&map.alias).map_err(|error| {
        ProjectError::Identity(format!(
            "registration diagnostic for alias {}: {error}",
            map.alias
        ))
    })?;
    if registration.project_id != map.project_id || registration.store_id != map.store_id {
        return Err(ProjectError::Identity(format!(
            "registration diagnostic for alias {} has wrong project/store identity",
            map.alias
        )));
    }
    store.bind_context(
        context_instance(&map.project_root, &map.metadata_root, &identity)?,
        Some(map.revision),
        map.project_root.clone(),
        Some(map.alias.clone()),
        cwd.to_path_buf(),
        map.recognized_remote_urls.clone(),
    );
    if let Some(diagnostic) = store.identity_diagnostic() {
        return Err(ProjectError::Identity(diagnostic));
    }
    Ok(())
}

pub fn select_context(
    cwd: &Path,
    explicit_root: Option<&Path>,
    explicit_metadata: Option<&Path>,
    alias: Option<&str>,
    allow_initialize: bool,
) -> Result<ProjectContext, ProjectError> {
    let all_maps = load_maps(cwd)?;
    let maps = all_maps.project.clone();
    let explicit_root = explicit_root
        .map(|path| absolute_lexical(cwd, path))
        .transpose()?;
    if let Some(root) = &explicit_root
        && !root.is_dir()
    {
        return Err(ProjectError::Location(format!(
            "explicit project root does not exist: {}",
            root.display()
        )));
    }

    let selected_map = match alias.filter(|alias| *alias != "root") {
        Some(alias) => Some(select_alias(&maps, alias, explicit_root.as_deref(), cwd)?),
        None => match explicit_root.as_deref() {
            Some(root) => exact_root_map(&maps, root)?,
            None => deepest_map(&maps, cwd)?,
        },
    };

    let (project_root, discovered_metadata) = if let Some(root) = explicit_root {
        (root, None)
    } else if let Some(map) = &selected_map {
        (map.project_root.clone(), None)
    } else if let Some(found) = discover_upward(cwd, &all_maps, allow_initialize)? {
        found
    } else if allow_initialize {
        (cwd.to_path_buf(), None)
    } else {
        return Err(ProjectError::Location("no project root found".into()));
    };

    let explicit_metadata = explicit_metadata
        .map(|path| absolute_lexical(cwd, path))
        .transpose()?;
    let metadata_was_explicit = explicit_metadata.is_some();
    let alias_was_explicit = alias.is_some_and(|alias| alias != "root");
    let mut mapped_for_root = if selected_map.is_some() {
        selected_map
    } else {
        exact_root_map(&maps, &project_root)?
    };
    if metadata_was_explicit && !alias_was_explicit {
        mapped_for_root = mapped_for_root.filter(|map| {
            explicit_metadata
                .as_ref()
                .is_some_and(|metadata| map.metadata_root == *metadata)
        });
    }

    let metadata_root = if let Some(path) = explicit_metadata {
        path
    } else if let Some(map) = &mapped_for_root {
        map.metadata_root.clone()
    } else if let Some(path) = discovered_metadata {
        path
    } else {
        choose_metadata(&project_root, &all_maps, allow_initialize)?
    };

    if !metadata_root.exists() {
        if let Some(map) = mapped_for_root.as_ref() {
            return Err(ProjectError::Location(format!(
                "mapped metadata directory does not exist for project {} store {}: {}",
                map.project_id,
                map.store_id,
                metadata_root.display()
            )));
        }
        if allow_initialize {
            return Ok(ProjectContext {
                project_root,
                metadata_root,
                metadata: MetadataSelection::Initialize,
                instance: String::new(),
                mapping_revision: None,
                alias: None,
                recognized_remote_urls: BTreeSet::new(),
            });
        }
        return Err(ProjectError::Location(format!(
            "metadata directory does not exist: {}",
            metadata_root.display()
        )));
    }

    if allow_initialize && configuration_only_bootstrap(&all_maps, &project_root, &metadata_root)? {
        return Ok(ProjectContext {
            project_root,
            metadata_root,
            metadata: MetadataSelection::InitializeConfigured,
            instance: String::new(),
            mapping_revision: None,
            alias: None,
            recognized_remote_urls: BTreeSet::new(),
        });
    }

    let identity =
        Store::identity_at(&metadata_root).map_err(|e| ProjectError::Location(e.to_string()))?;
    if mapped_for_root.is_none() && metadata_was_explicit && !alias_was_explicit {
        let candidates: Vec<_> = maps
            .iter()
            .filter(|map| {
                map.project_id == identity.project_id && map.store_id == identity.store_id
            })
            .cloned()
            .collect();
        if !candidates.is_empty() {
            let unique: BTreeSet<_> = candidates
                .iter()
                .map(|map| {
                    (
                        map.alias.clone(),
                        map.project_root.clone(),
                        map.metadata_root.clone(),
                    )
                })
                .collect();
            if unique.len() != 1 {
                return Err(ProjectError::Ambiguous(format!(
                    "project/store identity {}:{}",
                    identity.project_id, identity.store_id
                )));
            }
            mapped_for_root = candidates.into_iter().next();
        }
    }
    if let Some(map) = mapped_for_root.as_ref() {
        if map.project_id != identity.project_id || map.store_id != identity.store_id {
            return Err(ProjectError::Identity(format!(
                "mapping {} expects project {} store {}, found project {} store {}",
                map.alias, map.project_id, map.store_id, identity.project_id, identity.store_id
            )));
        }
        let registration = Store::open_existing(&metadata_root)
            .and_then(|store| store.project_registration(&map.alias))
            .map_err(|error| {
                ProjectError::Identity(format!(
                    "registration diagnostic for alias {}: {error}",
                    map.alias
                ))
            })?;
        if registration.project_id != identity.project_id
            || registration.store_id != identity.store_id
        {
            return Err(ProjectError::Identity(format!(
                "registration diagnostic for alias {} has wrong project/store identity",
                map.alias
            )));
        }
    }
    let instance = context_instance(&project_root, &metadata_root, &identity)?;
    Ok(ProjectContext {
        project_root,
        metadata_root,
        metadata: MetadataSelection::Existing(identity),
        instance,
        mapping_revision: mapped_for_root.as_ref().map(|map| map.revision),
        alias: mapped_for_root.as_ref().map(|map| map.alias.clone()),
        recognized_remote_urls: mapped_for_root
            .as_ref()
            .map(|map| map.recognized_remote_urls.clone())
            .unwrap_or_default(),
    })
}

fn select_alias(
    maps: &[ProjectMap],
    alias: &str,
    explicit_root: Option<&Path>,
    cwd: &Path,
) -> Result<ProjectMap, ProjectError> {
    let candidates: Vec<_> = maps
        .iter()
        .filter(|map| map.alias == alias)
        .cloned()
        .collect();
    if candidates.is_empty() {
        return Err(ProjectError::MissingAlias(alias.into()));
    }
    if let Some(root) = explicit_root {
        let matches: Vec<_> = candidates
            .into_iter()
            .filter(|map| map.project_root == root)
            .collect();
        return unique_map(matches, format!("alias {alias} at {}", root.display()));
    }
    let containing: Vec<_> = candidates
        .iter()
        .filter(|map| cwd.starts_with(&map.project_root))
        .cloned()
        .collect();
    if !containing.is_empty() {
        return deepest_unique(containing, format!("alias {alias}"));
    }
    unique_map(candidates, format!("alias {alias}"))
}

fn deepest_map(maps: &[ProjectMap], dir: &Path) -> Result<Option<ProjectMap>, ProjectError> {
    let candidates: Vec<_> = maps
        .iter()
        .filter(|map| dir.starts_with(&map.project_root))
        .cloned()
        .collect();
    if candidates.is_empty() {
        Ok(None)
    } else {
        deepest_unique(
            candidates,
            format!("registered roots containing {}", dir.display()),
        )
        .map(Some)
    }
}

fn exact_root_map(maps: &[ProjectMap], root: &Path) -> Result<Option<ProjectMap>, ProjectError> {
    let candidates: Vec<_> = maps
        .iter()
        .filter(|map| map.project_root == root)
        .cloned()
        .collect();
    if candidates.is_empty() {
        Ok(None)
    } else {
        unique_map(candidates, format!("registered root {}", root.display())).map(Some)
    }
}

fn deepest_unique(candidates: Vec<ProjectMap>, label: String) -> Result<ProjectMap, ProjectError> {
    let depth = candidates
        .iter()
        .map(|map| map.project_root.components().count())
        .max()
        .unwrap_or(0);
    unique_map(
        candidates
            .into_iter()
            .filter(|map| map.project_root.components().count() == depth)
            .collect(),
        label,
    )
}

fn unique_map(candidates: Vec<ProjectMap>, label: String) -> Result<ProjectMap, ProjectError> {
    let unique: BTreeSet<_> = candidates
        .iter()
        .map(|map| {
            (
                map.project_id.clone(),
                map.store_id.clone(),
                map.project_root.clone(),
                map.metadata_root.clone(),
            )
        })
        .collect();
    if unique.len() != 1 {
        return Err(ProjectError::Ambiguous(label));
    }
    Ok(candidates.into_iter().next().unwrap())
}

fn discover_upward(
    cwd: &Path,
    maps: &Maps,
    allow_initialize: bool,
) -> Result<Option<(PathBuf, Option<PathBuf>)>, ProjectError> {
    for root in cwd.ancestors() {
        let dot = root.join(".omd");
        if dot.exists() {
            if allow_initialize && configuration_only_bootstrap(maps, root, &dot)? {
                return Ok(Some((root.to_path_buf(), None)));
            }
            Store::identity_at(&dot).map_err(|e| ProjectError::Location(e.to_string()))?;
            return Ok(Some((root.to_path_buf(), Some(dot))));
        }
        let candidates = direct_manifest_candidates(root)?;
        if candidates.len() > 1 {
            return Err(ProjectError::Ambiguous(format!(
                "{} direct metadata candidates under {}",
                candidates.len(),
                root.display()
            )));
        }
        if let Some(metadata) = candidates.into_iter().next() {
            return Ok(Some((root.to_path_buf(), Some(metadata))));
        }
    }
    Ok(None)
}

fn choose_metadata(
    root: &Path,
    maps: &Maps,
    allow_initialize: bool,
) -> Result<PathBuf, ProjectError> {
    let dot = root.join(".omd");
    if dot.exists() {
        if allow_initialize && configuration_only_bootstrap(maps, root, &dot)? {
            return Ok(dot);
        }
        Store::identity_at(&dot).map_err(|e| ProjectError::Location(e.to_string()))?;
        return Ok(dot);
    }
    let candidates = direct_manifest_candidates(root)?;
    match candidates.len() {
        0 if allow_initialize => Ok(dot),
        0 => Err(ProjectError::Location(format!(
            "no metadata directory found for {}",
            root.display()
        ))),
        1 => Ok(candidates.into_iter().next().unwrap()),
        count => Err(ProjectError::Ambiguous(format!(
            "{count} direct metadata candidates under {}",
            root.display()
        ))),
    }
}

fn configuration_only_bootstrap(
    maps: &Maps,
    project_root: &Path,
    metadata_root: &Path,
) -> Result<bool, ProjectError> {
    if metadata_root != project_root.join(".omd") || !metadata_root.is_dir() {
        return Ok(false);
    }
    let mut entries = fs::read_dir(metadata_root)?;
    let Some(entry) = entries.next().transpose()? else {
        return Ok(false);
    };
    if entry.file_name() != "omd.toml"
        || !entry.path().is_file()
        || entries.next().transpose()?.is_some()
    {
        return Ok(false);
    }

    let canonical_project = fs::canonicalize(project_root)?;
    let canonical_metadata = fs::canonicalize(metadata_root)?;
    let binds_target = |mapped_project: &Path, mapped_metadata: &Path| {
        mapped_project == project_root
            || mapped_metadata == metadata_root
            || fs::canonicalize(mapped_project).is_ok_and(|path| path == canonical_project)
            || fs::canonicalize(mapped_metadata).is_ok_and(|path| path == canonical_metadata)
    };
    if maps
        .project
        .iter()
        .any(|map| binds_target(&map.project_root, &map.metadata_root))
        || maps
            .peer
            .iter()
            .any(|map| binds_target(&map.project_root, &map.metadata_root))
        || maps
            .authority
            .iter()
            .any(|map| binds_target(&map.project_root, &map.metadata_root))
    {
        return Err(ProjectError::Identity(format!(
            "configuration-only metadata target is already bound: {}",
            metadata_root.display()
        )));
    }
    Ok(true)
}

fn direct_manifest_candidates(root: &Path) -> Result<Vec<PathBuf>, ProjectError> {
    let mut candidates = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidates),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() && path.join("manifest.toml").exists() {
            Store::identity_at(&path).map_err(|e| ProjectError::Location(e.to_string()))?;
            candidates.push(path);
        }
    }
    candidates.sort();
    Ok(candidates)
}

/// Convert a CLI path into stable project-relative storage plus actual local
/// read path. Containment is lexical; symlinks remain followable by I/O.
pub fn project_path(root: &Path, input: &str) -> Result<(String, PathBuf), ProjectError> {
    let root = normalize(root)?;
    let input_path = Path::new(input);
    let absolute = if input_path.is_absolute() {
        normalize(input_path)?
    } else {
        normalize(&root.join(input_path))?
    };
    let relative = absolute.strip_prefix(&root).map_err(|_| {
        ProjectError::Location(format!(
            "path escapes selected project root {}: {input}",
            root.display()
        ))
    })?;
    if relative.as_os_str().is_empty() {
        return Ok((String::new(), absolute));
    }
    let logical = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    Ok((logical, absolute))
}

fn absolute_lexical(cwd: &Path, path: &Path) -> Result<PathBuf, ProjectError> {
    if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&cwd.join(path))
    }
}

fn normalize(path: &Path) -> Result<PathBuf, ProjectError> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(ProjectError::Location(format!(
                        "path escapes filesystem root: {}",
                        path.display()
                    )));
                }
            }
            Component::Normal(part) => result.push(part),
        }
    }
    Ok(result)
}
