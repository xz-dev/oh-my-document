//! `omd` — command-line entry to the OMD tracking core.
//!
//! Command surface follows the spec's commit-verb alias model: mutating
//! operations go through `omd commit <kind>`; top-level verbs like `init`,
//! `import`, `remove`, `delete`, `rename`, `copy` are aliases into those
//! commit kinds. `--json` selects a machine-readable envelope on every verb.

use clap::{Parser, Subcommand};

use omd::output::{Diagnostic, Envelope};
use omd::records::commit::CommitKind;
use omd::records::pipeline::{self, PipelineError};
use omd::records::store::{Expected, NoProbe, Store, StoreError};
use omd::records::time::{OsRng, SystemClock};
use omd::relations::identity::RefError;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LinkDirection {
    From,
    To,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EndpointArg {
    Local {
        direction: LinkDirection,
        commit: String,
    },
    Store {
        direction: LinkDirection,
        alias: String,
        commit: String,
    },
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AdaptArg {
    link_id: String,
    changes: Vec<String>,
    reason: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StopArg {
    link_id: String,
    changes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    Check,
    Usage,
    Version,
    Lock,
    IoExec,
}

impl ErrorKind {
    fn exit_code(self) -> u8 {
        match self {
            Self::Check => 1,
            Self::Usage => 2,
            Self::Version => 3,
            Self::Lock => 4,
            Self::IoExec => 5,
        }
    }

    fn diagnostic(self) -> &'static str {
        match self {
            Self::Check => "check_failed",
            Self::Usage => "usage",
            Self::Version => "version_conflict",
            Self::Lock => "lock_conflict",
            Self::IoExec => "io_exec",
        }
    }
}

#[derive(Debug)]
struct AppError {
    kind: ErrorKind,
    message: String,
    data: Option<serde_json::Value>,
}

impl AppError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            data: None,
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Usage, message)
    }

    fn version(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Version, message)
    }

    fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::IoExec, message)
    }

    fn partial(
        cause: AppError,
        succeeded_members: Vec<serde_json::Value>,
        failed_step: impl Into<String>,
        open_block: Vec<String>,
        operation_id: &str,
    ) -> Self {
        let failed_step = failed_step.into();
        let underlying_kind = cause.kind.diagnostic();
        let underlying_message = cause.message;
        let data = serde_json::json!({
            "kind": "combo_partial_failure",
            "succeeded_members": succeeded_members,
            "failed_step": failed_step,
            "open_block": open_block.clone(),
            "open_boundary": open_block,
            "operation_id": operation_id,
            "underlying_kind": underlying_kind,
            "underlying_error": {
                "kind": underlying_kind,
                "message": underlying_message.clone(),
            },
            "error": underlying_message,
        });
        Self {
            kind: cause.kind,
            message: data["error"]
                .as_str()
                .unwrap_or("publication failed")
                .to_string(),
            data: Some(data),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::Check, message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::new(ErrorKind::Check, message)
    }
}

impl From<StoreError> for AppError {
    fn from(error: StoreError) -> Self {
        let kind = match &error {
            StoreError::Lock => ErrorKind::Lock,
            StoreError::Conflict(_) => ErrorKind::Version,
            StoreError::Io(_) => ErrorKind::IoExec,
            StoreError::Record(_) => ErrorKind::Check,
        };
        Self::new(kind, error.to_string())
    }
}

impl From<PipelineError> for AppError {
    fn from(error: PipelineError) -> Self {
        match error {
            PipelineError::Store(error) => error.into(),
            PipelineError::Source(error) => Self::io(error.to_string()),
            PipelineError::Input(message) => Self::usage(message),
            PipelineError::Commit(message) => Self::new(ErrorKind::Check, message),
        }
    }
}

impl From<omd::sources::projects::ProjectError> for AppError {
    fn from(error: omd::sources::projects::ProjectError) -> Self {
        match error {
            omd::sources::projects::ProjectError::Conflict(message) => Self::version(message),
            omd::sources::projects::ProjectError::Store(error) => error.into(),
            omd::sources::projects::ProjectError::Io(error) => StoreError::Io(error).into(),
            other => Self::usage(other.to_string()),
        }
    }
}

impl From<omd::sources::SourceError> for AppError {
    fn from(error: omd::sources::SourceError) -> Self {
        match error {
            omd::sources::SourceError::Invalid(message) => Self::usage(message),
            other => Self::io(other.to_string()),
        }
    }
}

impl From<RefError> for AppError {
    fn from(error: RefError) -> Self {
        let message = error.to_string();
        match error {
            RefError::VersionMismatch(_, _) => Self::version(message),
            _ => Self::usage(message),
        }
    }
}

#[derive(Parser)]
#[command(name = "omd", version, about = "Object-tracking metadata store")]
struct Cli {
    /// Emit a JSON envelope on stdout (never mixed with stderr).
    #[arg(long, global = true)]
    json: bool,

    /// Explicit metadata directory (overrides discovery).
    #[arg(long, global = true, value_name = "DIR")]
    meta: Option<PathBuf>,

    /// Explicit project root. Relative values resolve against invocation cwd.
    #[arg(long, global = true, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Shared logical project alias resolved through machine-local placement.
    #[arg(long, global = true, value_name = "ALIAS")]
    project: Option<String>,

    /// Local registration name selecting metadata/store context.
    #[arg(long, global = true, value_name = "ALIAS")]
    store: Option<String>,

    /// Permit command sources to run during this verify/check
    /// (`--run-command` / `--run-command=false`). One call's flag never
    /// carries into another; built-in default is `false`. `require_equals`
    /// keeps a bare `--run-command` from greedily consuming the subcommand.
    #[arg(long, global = true, default_missing_value = "true",
         num_args = 0..=1, require_equals = true)]
    run_command: Option<bool>,

    /// Text encoding for this observation (`--encoding utf-16le`, etc.).
    /// Priority: flag > recorded > file config > project default > user
    /// default > UTF-8. A recorded encoding freezes that observation.
    #[arg(long, global = true)]
    encoding: Option<String>,

    /// Closed acquisition kind. New sources default to file; existing objects
    /// reuse their recorded observation definition when all source fields are omitted.
    #[arg(long = "source-type", global = true)]
    source_type: Option<String>,

    /// File/Git source project alias (`root` by default).
    #[arg(long = "source-project", global = true)]
    source_project: Option<String>,

    /// File source project-relative literal path.
    #[arg(long = "source-path", global = true)]
    source_path: Option<String>,

    /// Command source fixed executable.
    #[arg(long, global = true)]
    executable: Option<String>,

    /// Command source literal JSON string-array argv.
    #[arg(long = "args-json", global = true)]
    args_json: Option<String>,

    /// Git source complete exact object id.
    #[arg(long = "git-commit", global = true)]
    git_commit: Option<String>,

    /// Git source literal path within that commit.
    #[arg(long = "git-path", global = true)]
    git_path: Option<String>,

    /// Observation credential JSON or path to a JSON file. Required for
    /// every write to an existing store/object.
    #[arg(long, global = true, value_name = "JSON_OR_FILE")]
    expected: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

/// `omd project …` subcommands — machine-local project mappings.
#[derive(Subcommand)]
enum ProjectAction {
    /// `omd project register <alias> <project-root> <metadata-dir>`.
    Register {
        alias: String,
        project_root: String,
        metadata_dir: String,
        #[arg(long = "git-remote", requires = "git_remote_url")]
        git_remote: Option<String>,
        #[arg(long = "git-remote-url", requires = "git_remote")]
        git_remote_url: Option<String>,
    },
    /// Explicitly approve one raw local remote URL for this exact mapping.
    RecognizeRemote {
        alias: String,
        #[arg(long = "git-remote-url")]
        git_remote_url: String,
    },
    /// `omd project list` — show registered project_root ↔ metadata_root.
    List,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Cmd {
    /// `commit init` — first record for a source.
    Init { path: String },
    /// `commit <kind>` — ordinary/marker/lifecycle commits.
    Commit {
        kind: String,
        path: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, num_args = 2, value_names = ["START", "END"])]
        range: Option<Vec<u64>>,
        /// Coordinate unit: `text` (Unicode scalars) or `byte` (raw offsets).
        /// Defaults to text; `--range 0 5 --mode byte` = byte-offset span.
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        timestamp: Option<String>,
        /// `commit reset <path> --target <commit-id>` — the commit to reset to.
        #[arg(long)]
        reset_target: Option<String>,
        /// Explicit link endpoint commit IDs; optional store fields select registered peers.
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long = "source-store")]
        source_store: Option<String>,
        #[arg(long = "target-store")]
        target_store: Option<String>,
        /// `commit clean --stop '<JSON {link_id,changes}>'` — source-side branch stop.
        #[arg(long)]
        stop: Vec<String>,
        /// `--adapt '<JSON {link_id,changes,reason}>'` — repeatable explicit selections.
        #[arg(long)]
        adapt: Vec<String>,
        /// `commit clean --no--reason` — explicitly omit the stop reason.
        #[arg(long = "no--reason")]
        no_reason: bool,
        /// `commit ... --link-from R` — create R→this link in the block.
        #[arg(long = "link-from")]
        link_from: Vec<String>,
        /// `commit ... --link-to R` — create this→R link in the block.
        #[arg(long = "link-to")]
        link_to: Vec<String>,
        /// Structured cross-store incoming endpoint.
        #[arg(long = "link-from-store", num_args = 2, value_names = ["STORE", "ID"])]
        link_from_store: Vec<String>,
        /// Structured cross-store outgoing endpoint.
        #[arg(long = "link-to-store", num_args = 2, value_names = ["STORE", "ID"])]
        link_to_store: Vec<String>,
        /// `--id <commit>` — append to an existing range chain vs create a new
        /// one (same coords without --id = a new independent range object).
        #[arg(long)]
        id: Option<String>,
        /// `commit tag <path> --tag <name>` — flat project-local tag.
        #[arg(long)]
        tag: Option<String>,
        /// `commit scope_adjust --rule "spec->code" --level fail` — declare a
        /// named tag-link rule checked by `check`.
        #[arg(long)]
        rule: Option<String>,
        /// Rule severity: `warn` or `fail` (default `fail`).
        #[arg(long)]
        level: Option<String>,
        /// Explicitly skip the named rule — skip never confirms content.
        #[arg(long)]
        skip: bool,
        /// Assert the node's current effective tip equals this commit id
        /// before writing (optimistic-concurrency guard — a mismatch is a
        /// version conflict, never a silent overwrite).
        #[arg(long = "expect-version")]
        expect_version: Option<String>,
    },
    /// `omd verify` — full-store check (distinct from `commit verify <path>`).
    Verify { path: Option<String> },
    /// `omd check` — content/link coverage report.
    Check { path: Option<String> },
    /// `omd import` — alias for `commit import` (statistics inclusion).
    /// Includes subdirs; `--exclude`/`--include` layer patterns over the scope.
    Import {
        path: String,
        #[arg(long = "exclude")]
        exclude: Vec<String>,
        #[arg(long = "include")]
        include: Vec<String>,
    },
    /// `omd remove` — alias for `commit remove` (statistics removal).
    Remove { path: String },
    /// `omd delete` — alias for `commit delete` (tombstone).
    Delete { path: String },
    /// `omd rename` — alias for `commit rename` (source→target).
    Rename { source: String, target: String },
    /// `omd copy` — new initialization without prior association.
    Copy { source: String, target: String },
    /// `omd note` — append-only notes on a commit + reverse lookup.
    Note {
        /// `add|patch|delete|list`
        action: String,
        /// Target commit id.
        commit_id: String,
        /// For patch/delete: the original note id.
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        text: Option<String>,
    },
    /// Rebind one commit's complete source version recovery definition.
    Replace { commit_id: String },
    /// `omd register <peer-store-id> <project-root> <metadata-dir>
    /// --peer-expected <JSON>` — validate caller-observed real peer,
    /// publish immutable logical registration, and save physical placement
    /// only in machine-local configuration.
    Register {
        store_id: String,
        project_root: String,
        metadata_dir: String,
        #[arg(long = "peer-expected")]
        peer_expected: String,
    },
    /// `omd project register <alias> <project-root> <metadata-dir>` — bind a
    /// shared logical alias to this machine's placement (machine-local
    /// `projects.toml`, never the shared store).
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// `omd protect <target> --peer <peer-store-id> --record <commit-id>
    /// --link <link-id>` — persist an inbound protection credential before
    /// the peer's exact planned record publishes.
    Protect {
        target: String,
        #[arg(long)]
        peer: String,
        #[arg(long)]
        record: String,
        #[arg(long)]
        link: String,
    },
    /// `omd activate` — activate a copied store: new store_id + completed
    /// external-reference registration. Unactivated copies are read-only.
    Activate,
    /// `omd gc [--content]` — free contents no longer protected. Without
    /// --content only metadata is collected; --content releases local content
    /// copies with no retention record, rechecking Git-replacement basis.
    Gc {
        /// Also release local content copies.
        #[arg(long)]
        content: bool,
    },
    /// `omd reindex` — rebuild the rebuildable query index from the
    /// authoritative `published` manifest. Never re-fetches source content
    /// or Git objects — the index is a derived cache, not a second authority.
    Reindex,
    /// `omd log <id>` — walk a node's commit chain.
    Log { id: String },
    /// `omd tree [id] [--level N]` — mount-tree view from root or a node.
    /// `--level file` limits to file level (no range children); `--level N`
    /// caps depth.
    Tree {
        id: Option<String>,
        /// `file` = stop at file level; a number = max depth.
        #[arg(long)]
        level: Option<String>,
    },
    /// `omd list` — query state (e.g. `--dangling`).
    List {
        #[arg(long)]
        dangling: bool,
    },
}

/// Read a commit's previous_id from its on-disk record.
fn commit_prev(root: &Path, id: &str) -> Option<String> {
    let s = std::fs::read_to_string(root.join(format!("commits/{id}.toml"))).ok()?;
    let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
    Some(c.previous_id)
}

/// Walk a live object key or retained commit (full/unique prefix) tip→root.
fn log_chain(store: &Store, node_or_commit: &str) -> Result<Vec<String>, AppError> {
    let mut current = if let Some(tip) = store.state().tips.get(node_or_commit) {
        tip.clone()
    } else {
        omd::relations::identity::resolve_commit_id(store, node_or_commit)
            .map_err(|e| AppError::usage(e.to_string()))?
    };
    let mut out = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    while !current.is_empty() {
        if !visited.insert(current.clone()) {
            return Err(AppError::usage(format!(
                "cycle in commit ancestry at {current}"
            )));
        }
        out.push(current.clone());
        current = store
            .read_commit(&current)
            .map_err(|e| AppError::usage(format!("incomplete commit ancestry: {e}")))?
            .previous_id;
    }
    Ok(out)
}

/// Resolve the latest effective import/remove through later metadata records.
fn import_scope(store: &Store, node: &str) -> Result<Option<Vec<String>>, AppError> {
    let chain = log_chain(store, node)?;
    for id in chain {
        let record = store.read_commit(&id).map_err(|error| {
            AppError::new(ErrorKind::Check, format!("import record {id}: {error}"))
        })?;
        match record.kind {
            CommitKind::Remove => return Ok(None),
            CommitKind::Import => {
                let patterns = record
                    .payload
                    .get("scope")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                return Ok(Some(patterns));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Render the mount tree as nested JSON from `start` (or the implicit root).
/// Tree shows mount hierarchy: root → file nodes → range children. Only
/// mounted nodes expand — never unpublished material as history.
/// Resolve explicit metadata input without treating a malformed/nonexistent
/// value as unset. Relative values remain relative to invocation cwd until
/// project selection normalizes them.
fn explicit_metadata(cli: &Cli) -> Option<PathBuf> {
    cli.meta.clone().or_else(|| {
        std::env::var("OMD_META")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    })
}

fn source_fields(cli: &Cli) -> omd::sources::reference::SourceFields {
    omd::sources::reference::SourceFields {
        source_type: cli.source_type.clone(),
        source_project: cli.source_project.clone(),
        source_path: cli.source_path.clone(),
        executable: cli.executable.clone(),
        args_json: cli.args_json.clone(),
        git_commit: cli.git_commit.clone(),
        git_path: cli.git_path.clone(),
    }
}

fn source_is_byte_mode(cli: &Cli) -> bool {
    matches!(
        &cli.cmd,
        Cmd::Commit { mode, .. } if mode.as_deref() == Some("byte")
    )
}

fn preflight_source_fields(cli: &Cli) -> Result<(), AppError> {
    if let Some(encoding) = cli.encoding.as_deref()
        && !source_is_byte_mode(cli)
    {
        omd::sources::encoding::validate(encoding)
            .map_err(|error| AppError::usage(error.to_string()))?;
    }
    let fields = source_fields(cli);
    let accepts = matches!(
        cli.cmd,
        Cmd::Init { .. } | Cmd::Commit { .. } | Cmd::Replace { .. }
    );
    if fields.any() && !accepts {
        return Err(AppError::usage(
            "source fields are accepted only by init, commit, and replace",
        ));
    }
    if matches!(cli.cmd, Cmd::Replace { .. }) {
        fields
            .descriptor(None, true)
            .map_err(|error| AppError::usage(error.to_string()))?;
    } else if fields.any() {
        fields
            .descriptor(Some("__preflight__"), false)
            .map_err(|error| AppError::usage(error.to_string()))?;
    }
    Ok(())
}

fn initializes_store(cmd: &Cmd) -> bool {
    matches!(cmd, Cmd::Init { .. }) || matches!(cmd, Cmd::Commit { kind, .. } if kind == "init")
}

fn open_initial_store(context: &omd::sources::projects::ProjectContext) -> Result<Store, AppError> {
    match &context.metadata {
        omd::sources::projects::MetadataSelection::Initialize => {
            Ok(Store::open(&context.metadata_root)?)
        }
        omd::sources::projects::MetadataSelection::InitializeConfigured => {
            Ok(Store::open_configured(&context.metadata_root)?)
        }
        omd::sources::projects::MetadataSelection::Existing(_) => {
            Err(AppError::usage("project metadata is already initialized"))
        }
    }
}

/// Resolve one validated project root + metadata instance before source
/// acquisition or business writes.
fn project_context(cli: &Cli) -> Result<omd::sources::projects::ProjectContext, AppError> {
    let cwd = std::env::current_dir()
        .map_err(|error| AppError::io(format!("current directory: {error}")))?;
    let alias = match (cli.store.as_deref(), cli.project.as_deref()) {
        (Some(store), Some(project)) if store == "root" => Some(project),
        (Some(store), Some(project)) if project == "root" => Some(store),
        (Some(store), Some(project)) if store != project => {
            return Err(AppError::usage(
                "--store and --project must select the same registered context when both are given",
            ));
        }
        (Some(store), _) => Some(store),
        (_, project) => project,
    };
    Ok(omd::sources::projects::select_context(
        &cwd,
        cli.root.as_deref(),
        explicit_metadata(cli).as_deref(),
        alias,
        initializes_store(&cli.cmd),
    )?)
}

fn parse_expected_arg(raw: &str) -> Result<Expected, AppError> {
    let text = if raw.trim_start().starts_with('{') {
        raw.to_string()
    } else {
        std::fs::read_to_string(raw).map_err(|error| {
            AppError::usage(format!("cannot read expected evidence file {raw}: {error}"))
        })?
    };
    serde_json::from_str(&text)
        .map_err(|error| AppError::usage(format!("invalid expected evidence: {error}")))
}

fn expected_from_cli(cli: &Cli) -> Result<Expected, AppError> {
    let raw = cli.expected.as_deref().ok_or_else(|| {
        AppError::usage("existing-object write requires --expected <JSON_OR_FILE>")
    })?;
    parse_expected_arg(raw)
}

fn open_context_store(context: &omd::sources::projects::ProjectContext) -> Result<Store, AppError> {
    let cwd = std::env::current_dir()
        .map_err(|error| AppError::io(format!("current directory: {error}")))?;
    let mut store = Store::open_existing(&context.metadata_root)?;
    store.bind_context(
        context.instance.clone(),
        context.mapping_revision,
        context.project_root.clone(),
        context.alias.clone(),
        cwd,
        context.recognized_remote_urls.clone(),
    );
    Ok(store)
}

fn observation(
    store: &Store,
    context: &omd::sources::projects::ProjectContext,
) -> Result<Expected, AppError> {
    let mut expected =
        Expected::observe(store, context.instance.clone(), context.mapping_revision)?;
    if let Some(cwd) = store.config_cwd() {
        for (peer_store_id, registration_id) in &store.state().peers {
            let Ok(mapping) =
                omd::sources::projects::peer_mapping(cwd, &store.state().store_id, peer_store_id)
            else {
                continue;
            };
            if mapping.peer_registration_id != *registration_id {
                continue;
            }
            let Ok(identity) = Store::identity_at(&mapping.metadata_root) else {
                continue;
            };
            if identity.store_id != *peer_store_id || identity.project_id != mapping.peer_project_id
            {
                continue;
            }
            let Ok(peer) = Store::open_existing(&mapping.metadata_root) else {
                continue;
            };
            expected.peers.insert(
                peer_store_id.clone(),
                omd::records::store::PeerExpected {
                    instance: omd::sources::projects::context_instance(
                        &mapping.project_root,
                        &mapping.metadata_root,
                        &identity,
                    )?,
                    mapping_revision: mapping.revision,
                    project_id: identity.project_id,
                    store_id: identity.store_id,
                    publication: peer.state().publication,
                    tips: peer.state().tips.clone(),
                    registrations: peer.state().registrations.clone(),
                    peers: peer.state().peers.clone(),
                },
            );
        }
    }
    Ok(expected)
}

fn persist_successful_observations(
    store: &mut Store,
    context: &omd::sources::projects::ProjectContext,
    observations: &std::collections::BTreeMap<String, pipeline::SuccessfulObservation>,
) -> Result<Expected, AppError> {
    let mut expected = observation(store, context)?;
    for (node, captured) in observations {
        let basis = store.source_version_id(node)?.ok_or_else(|| {
            omd::records::store::StoreError::Conflict(format!(
                "source basis for {node} is unavailable"
            ))
        })?;
        let mut id = [0u8; 16];
        omd::testing::Rng::fill(&OsRng, &mut id);
        let version = omd::records::version::SourceVersion::new(
            omd::records::ids::Id128(id),
            &captured.bytes,
            captured.acquisition.clone(),
            captured.encoding.clone(),
        );
        let stored = store.persist_observation(&version, &captured.bytes)?;
        expected.basis_versions.insert(node.clone(), basis);
        expected
            .source_versions
            .insert(node.clone(), stored.id.to_hex());
        expected
            .acquisition_versions
            .insert(node.clone(), stored.id.to_hex());
        expected
            .source_hashes
            .insert(node.clone(), stored.sha256.clone());
    }
    Ok(expected)
}

/// Resolve a local link endpoint arg to its canonical range node key.
///
/// Cross-store endpoints are accepted only by the grouped store+commit
/// options.
fn resolve_range_endpoint(store: &Store, r: &str) -> Result<(String, String), AppError> {
    if r.starts_with("peer:") {
        return Err(AppError::usage(format!(
            "local link endpoint cannot use peer: spelling; use a grouped cross-store option: {r}"
        )));
    }
    // `range:<id>` — a node id (full or unique prefix). `range:file@span`
    // is a coordinate spelling, not an object id — rejected with the
    // coordinate rule below.
    if let Some(id) = r.strip_prefix("range:")
        && id.find('@').is_none()
    {
        let key = resolve_node_by_id(store, id, "range")?;
        let version =
            store.state().tips.get(&key).cloned().ok_or_else(|| {
                AppError::usage(format!("range node has no current version: {key}"))
            })?;
        return Ok((key, version));
    }
    // `file@span` / `range:file@span` coordinate spellings are NOT legal
    // endpoints (spec: "不靠 path+range 选择对象"; "旧路径拼坐标端点接口
    // 不提供兼容解释"). Position is never an identity — the caller must
    // name the range object by its chain-root id or a commit on the chain.
    if r.find('@').is_some() {
        return Err(AppError::usage(format!(
            "link endpoint must be a range object id (range:<id> or commit id), not a path+span coordinate: {r}"
        )));
    }
    // Bare commit id (64-hex) or unique prefix: find the chain containing it.
    if r.chars().all(|c| c.is_ascii_hexdigit()) && r.len() >= 8 {
        match omd::relations::identity::commit_to_node(store, r) {
            Ok((key, _root)) if key.starts_with("range:") => {
                let selected = omd::relations::identity::resolve_commit_id(store, r)
                    .map_err(|e| AppError::usage(e.to_string()))?;
                return Ok((key, selected));
            }
            Ok((key, _)) => {
                return Err(AppError::usage(format!(
                    "link endpoint resolves to a non-range node ({key}): {r}"
                )));
            }
            Err(_) => {}
        }
        // Also try as a node-id prefix across kinds.
        if let Ok(k) = resolve_node_by_id(store, r, "range") {
            let version = store.state().tips.get(&k).cloned().ok_or_else(|| {
                AppError::usage(format!("range node has no current version: {k}"))
            })?;
            return Ok((k, version));
        }
        return Err(AppError::usage(format!(
            "link endpoint does not resolve to a node: {r}"
        )));
    }
    Err(AppError::usage(format!(
        "link endpoint must name a range object (range:<id> or commit id), got: {r}"
    )))
}

#[derive(Debug, Clone)]
struct ResolvedEndpoint {
    direction: LinkDirection,
    actual_store_id: String,
    root: String,
    version: String,
    peer_store_id: Option<String>,
    label: String,
}

fn ordered_endpoint_args() -> Result<Vec<EndpointArg>, AppError> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let mut endpoints = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let take = |offset: usize| {
            args.get(index + offset)
                .cloned()
                .ok_or_else(|| AppError::usage(format!("{} is missing a value", args[index])))
        };
        match args[index].as_str() {
            "--" => break,
            "--link-from" => {
                endpoints.push(EndpointArg::Local {
                    direction: LinkDirection::From,
                    commit: take(1)?,
                });
                index += 2;
            }
            "--link-to" => {
                endpoints.push(EndpointArg::Local {
                    direction: LinkDirection::To,
                    commit: take(1)?,
                });
                index += 2;
            }
            "--link-from-store" => {
                endpoints.push(EndpointArg::Store {
                    direction: LinkDirection::From,
                    alias: take(1)?,
                    commit: take(2)?,
                });
                index += 3;
            }
            "--link-to-store" => {
                endpoints.push(EndpointArg::Store {
                    direction: LinkDirection::To,
                    alias: take(1)?,
                    commit: take(2)?,
                });
                index += 3;
            }
            "--range" => index += 3,
            "--json" | "--skip" | "--no--reason" | "--content" | "--dangling" => index += 1,
            "--root" | "--meta" | "--project" | "--store" | "--encoding" | "--source-type"
            | "--source-project" | "--source-path" | "--executable" | "--args-json"
            | "--git-commit" | "--git-path" | "--expected" | "--reason" | "--mode"
            | "--timestamp" | "--reset-target" | "--source" | "--target" | "--source-store"
            | "--target-store" | "--adapt" | "--stop" | "--id" | "--tag" | "--rule" | "--level"
            | "--expect-version" | "--peer-expected" | "--git-remote" | "--git-remote-url"
            | "--target-store-id" | "--peer" | "--record" | "--link" => index += 2,
            value if value.starts_with("--run-command=") => index += 1,
            _ => index += 1,
        }
    }
    Ok(endpoints)
}

fn peer_store_id_for_alias(store: &Store, alias: &str) -> Result<String, AppError> {
    let cwd = store.config_cwd().ok_or_else(|| {
        AppError::usage("store alias resolution requires machine-local configuration")
    })?;
    let ids: std::collections::BTreeSet<String> = omd::sources::projects::load(cwd)?
        .into_iter()
        .filter(|mapping| mapping.alias == alias)
        .map(|mapping| mapping.store_id)
        .collect();
    let store_id = match ids.len() {
        1 => ids.into_iter().next().unwrap(),
        0 if store.state().peers.contains_key(alias) => alias.to_string(),
        0 => {
            return Err(AppError::usage(format!(
                "store alias is not registered: {alias}"
            )));
        }
        _ => {
            return Err(AppError::usage(format!(
                "store alias is ambiguous: {alias}"
            )));
        }
    };
    if store_id == store.state().store_id {
        return Err(AppError::usage(format!(
            "external endpoint alias selects the current store: {alias}"
        )));
    }
    Ok(store_id)
}

/// Assert a peer store is registered and `ptarget` names a live range
/// node in it. Used for pre-BEGIN endpoint validation — a missing peer or
/// phantom target is a usage error, not a mid-block failure.
fn peer_store(store: &Store, psid: &str, expected: &Expected) -> Result<Store, AppError> {
    let cwd = store
        .config_cwd()
        .ok_or_else(|| AppError::usage("peer resolution requires machine-local configuration"))?;
    let registration_id = store
        .state()
        .peers
        .get(psid)
        .ok_or_else(|| AppError::usage(format!("peer store not registered: {psid}")))?;
    let mapping = omd::sources::projects::peer_mapping(cwd, &store.state().store_id, psid)?;
    if mapping.peer_registration_id != *registration_id {
        return Err(AppError::version(format!(
            "peer mapping registration changed: {psid}"
        )));
    }
    let path = mapping.metadata_root.clone();
    if !path.join("state.toml").is_file() {
        return Err(AppError::io(format!(
            "peer store metadata is missing at {}",
            path.display()
        )));
    }
    let peer = Store::open_existing(&path)
        .map_err(|e| AppError::io(format!("peer store unreadable at {}: {e}", path.display())))?;
    let observed = expected.peers.get(psid).ok_or_else(|| {
        AppError::version(format!(
            "caller has no successful peer observation for {psid}; rerun verify/check"
        ))
    })?;
    let live_instance = omd::sources::projects::context_instance(
        &mapping.project_root,
        &mapping.metadata_root,
        &peer.identity(),
    )?;
    if observed.instance != live_instance
        || observed.mapping_revision != mapping.revision
        || observed.project_id != peer.identity().project_id
        || observed.store_id != peer.state().store_id
        || observed.publication != peer.state().publication
        || observed.tips != peer.state().tips
        || observed.registrations != peer.state().registrations
        || observed.peers != peer.state().peers
    {
        return Err(AppError::version(format!(
            "peer observation is stale or incomplete: {psid}"
        )));
    }
    let peer_registration = omd::records::cross::read_peer(store, registration_id)?;
    if peer.state().store_id != psid
        || peer.identity().project_id != peer_registration.peer_project_id
        || mapping.peer_store_id != psid
        || mapping.peer_project_id != peer_registration.peer_project_id
    {
        return Err(AppError::usage(format!(
            "peer store identity mismatch: registered {psid}, found {} at {}",
            peer.state().store_id,
            path.display()
        )));
    }
    Ok(peer)
}

fn lock_participants(
    local: &mut Store,
    peers: &mut std::collections::BTreeMap<String, Store>,
) -> Result<(), AppError> {
    let mut order: Vec<(std::path::PathBuf, Option<String>)> = Vec::new();
    order.push((
        std::fs::canonicalize(local.root())
            .map_err(|error| AppError::io(format!("local metadata location: {error}")))?,
        None,
    ));
    for (store_id, peer) in peers.iter() {
        order.push((
            std::fs::canonicalize(peer.root())
                .map_err(|error| AppError::io(format!("peer metadata location: {error}")))?,
            Some(store_id.clone()),
        ));
    }
    order.sort_by(|left, right| left.0.cmp(&right.0));
    if order.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(AppError::usage(
            "cross-store participants resolve to the same metadata directory",
        ));
    }
    for (_, store_id) in order {
        match store_id {
            None => local.lock()?,
            Some(store_id) => peers
                .get_mut(&store_id)
                .ok_or_else(|| AppError::usage("peer participant disappeared"))?
                .lock()?,
        }
    }
    Ok(())
}

fn validate_peer_locked(
    owner: &Store,
    peer: &mut Store,
    observed: &omd::records::store::PeerExpected,
) -> Result<(), AppError> {
    omd::records::cross::validate_peer_locked(owner, peer, &observed.store_id, observed)?;
    Ok(())
}

fn resolve_peer_range_commit(
    store: &Store,
    psid: &str,
    commit_id: &str,
    expected: &Expected,
) -> Result<(String, String), AppError> {
    let peer = peer_store(store, psid, expected)?;
    let selected = omd::relations::identity::resolve_commit_id(&peer, commit_id)
        .map_err(|e| AppError::usage(format!("peer commit does not resolve: {commit_id}: {e}")))?;
    let (node, _) = omd::relations::identity::commit_to_node(&peer, &selected)
        .map_err(|e| AppError::usage(format!("peer commit does not resolve: {commit_id}: {e}")))?;
    if !node.starts_with("range:") {
        return Err(AppError::usage(format!(
            "peer commit does not resolve to a range: {commit_id}"
        )));
    }
    Ok((node, selected))
}

fn resolve_endpoint_arg(
    store: &Store,
    endpoint: EndpointArg,
    expected: &Expected,
) -> Result<ResolvedEndpoint, AppError> {
    match endpoint {
        EndpointArg::Local { direction, commit } => {
            let (root, version) = resolve_range_endpoint(store, &commit)
                .map_err(|error| AppError::usage(format!("local endpoint {commit}: {error}")))?;
            Ok(ResolvedEndpoint {
                direction,
                actual_store_id: store.state().store_id.clone(),
                root,
                version,
                peer_store_id: None,
                label: commit,
            })
        }
        EndpointArg::Store {
            direction,
            alias,
            commit,
        } => {
            let store_id = peer_store_id_for_alias(store, &alias)?;
            let (root, version) = resolve_peer_range_commit(store, &store_id, &commit, expected)?;
            Ok(ResolvedEndpoint {
                direction,
                actual_store_id: store_id.clone(),
                root,
                version,
                peer_store_id: Some(store_id),
                label: format!("{alias} {commit}"),
            })
        }
    }
}

/// Resolve a node id (full or unique prefix) of `kind` to its state key.
fn resolve_node_by_id(store: &Store, id: &str, kind: &str) -> Result<String, AppError> {
    let exact = format!("{kind}:{id}");
    if store.state().tips.contains_key(&exact) {
        return Ok(exact);
    }
    let prefix = format!("{kind}:{id}");
    let matches: Vec<&String> = store
        .state()
        .tips
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .collect();
    match matches.len() {
        1 => Ok(matches[0].clone()),
        0 => Err(AppError::usage(format!("{kind} node does not exist: {id}"))),
        _ => Err(AppError::usage(format!("ambiguous {kind} id prefix: {id}"))),
    }
}

fn mount_tree(
    store: &Store,
    start: Option<&str>,
    max_depth: usize,
    file_only: bool,
) -> Result<serde_json::Value, AppError> {
    fn build(
        store: &Store,
        node: &str,
        depth: usize,
        max_depth: usize,
        file_only: bool,
    ) -> Result<serde_json::Value, AppError> {
        let tip = store
            .state()
            .tips
            .get(node)
            .ok_or_else(|| AppError::usage(format!("unknown tree node: {node}")))?;
        if depth > max_depth {
            return Ok(serde_json::json!({
                "object": object_projection(store, node, tip, true)?,
                "truncated": true
            }));
        }
        let children: Result<Vec<_>, _> = store
            .state()
            .mounts
            .get(node)
            .into_iter()
            .flatten()
            .filter(|child| !(file_only && omd::relations::node::is_range_key(child)))
            .map(|child| build(store, child, depth + 1, max_depth, file_only))
            .collect();
        Ok(serde_json::json!({
            "node": node,
            "object": object_projection(store, node, tip, true)?,
            "path": omd::relations::node::path_of(store.state(), node),
            "children": children?,
        }))
    }
    let root = start.unwrap_or("root");
    if root != "root" {
        let node = if store.state().tips.contains_key(root) {
            root.to_string()
        } else {
            omd::relations::identity::commit_to_node(store, root)
                .map_err(|error| AppError::usage(error.to_string()))?
                .0
        };
        return Ok(serde_json::json!({
            "ok": true,
            "tree": build(store, &node, 0, max_depth, file_only)?,
        }));
    }
    let mounted: std::collections::BTreeSet<_> =
        store.state().mounts.values().flatten().cloned().collect();
    let children: Result<Vec<_>, _> = store
        .state()
        .tips
        .keys()
        .filter(|node| omd::relations::node::is_file_key(node) && !mounted.contains(*node))
        .map(|node| build(store, node, 1, max_depth, file_only))
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "tree": {
            "node": "root",
            "object": {
                "scope": local_scope(store),
                "kind": "root",
                "root_commit_id": null
            },
            "path": null,
            "children": children?,
        },
    }))
}

/// Commits that are dangling: recorded in `retained` but no longer a tip and
/// not reachable as an ancestor of any current tip — reset withdrew them.
fn dangling_ids(store: &Store, root: &Path) -> Vec<String> {
    let st = store.state();
    // Reachable = every tip plus its previous_id ancestors (walk the chain).
    let mut reachable = std::collections::BTreeSet::new();
    for tip in st.tips.values() {
        let mut cur = tip.clone();
        while !cur.is_empty() && reachable.insert(cur.clone()) {
            cur = commit_prev(root, &cur).unwrap_or_default();
        }
    }
    st.retained
        .iter()
        .filter(|id| !reachable.contains(*id))
        .cloned()
        .collect()
}

fn local_scope(store: &Store) -> serde_json::Value {
    serde_json::json!({ "kind": "local", "store_id": store.state().store_id })
}

fn object_ref_value(store: &Store, node: &str) -> serde_json::Value {
    if let Some(rest) = node.strip_prefix("peer:")
        && let Some((store_id, remote)) = rest.split_once(':')
        && let Some((kind, root)) = remote.split_once(':')
    {
        return serde_json::json!({
            "scope": { "kind": "store", "store_id": store_id },
            "kind": kind,
            "root_commit_id": root,
        });
    }
    let (kind, root) = node.split_once(':').unwrap_or(("unknown", node));
    serde_json::json!({
        "scope": local_scope(store),
        "kind": kind,
        "root_commit_id": root,
    })
}

fn historical_source_and_position(
    store: &Store,
    selected: &str,
) -> Result<
    (
        Option<String>,
        Option<String>,
        Option<omd::relations::range::Range>,
        Option<String>,
    ),
    AppError,
> {
    let mut current = selected.to_string();
    let mut visited = std::collections::BTreeSet::new();
    let mut effective_commit = None;
    let mut source_version = None;
    let mut position = None;
    let mut path = None;
    while !current.is_empty() {
        if !visited.insert(current.clone()) {
            return Err(AppError::usage(format!(
                "cycle in commit ancestry at {current}"
            )));
        }
        let commit = store
            .read_commit(&current)
            .map_err(|error| AppError::usage(format!("incomplete commit ancestry: {error}")))?;
        if path.is_none() {
            path = commit
                .payload
                .get("target")
                .or_else(|| commit.payload.get("path"))
                .and_then(|value| value.as_str())
                .map(str::to_string);
        }
        if source_version.is_none() && commit.content_ref != "empty" {
            source_version = Some(commit.content_ref.clone());
        }
        if position.is_none()
            && let Some(range) = commit
                .payload
                .get("position")
                .and_then(omd::relations::node::position_from_value)
        {
            effective_commit = Some(current.clone());
            position = Some(range);
        }
        current = commit.previous_id;
    }
    Ok((effective_commit, source_version, position, path))
}

fn object_projection(
    store: &Store,
    node: &str,
    selected: &str,
    current_location: bool,
) -> Result<serde_json::Value, AppError> {
    let object = object_ref_value(store, node);
    let tip = store.state().tips.get(node).cloned();
    let (effective_commit, source_version, position, recorded_path) =
        historical_source_and_position(store, selected)?;
    let reference_kind = node
        .split_once(':')
        .map(|(kind, _)| kind)
        .unwrap_or("unknown");
    let object_kind = if reference_kind == "file" {
        node.split_once(':')
            .and_then(|(_, root)| store.read_commit(root).ok())
            .map(|commit| {
                if commit.kind == CommitKind::Import {
                    "import"
                } else {
                    "file"
                }
            })
            .unwrap_or("file")
    } else {
        reference_kind
    };
    let (owning_file, path) = if reference_kind == "range" {
        let parent = omd::relations::identity::authoritative_object(store, selected)
            .map_err(|error| AppError::usage(error.to_string()))?
            .parent;
        let path = if current_location {
            parent
                .as_deref()
                .and_then(|parent| omd::relations::node::path_of(store.state(), parent))
                .map(str::to_string)
        } else {
            recorded_path
        };
        (parent.map(|parent| object_ref_value(store, &parent)), path)
    } else {
        let path = if current_location {
            omd::relations::node::path_of(store.state(), node).map(str::to_string)
        } else {
            omd::relations::identity::authoritative_object(store, selected)
                .map_err(|error| AppError::usage(error.to_string()))?
                .location
        };
        (None, path)
    };
    let source_version = match source_version.as_deref() {
        Some(version_id) => match store.read_version(version_id) {
            Ok(version) => Some(serde_json::json!({
                "status": "available",
                "version_id": version_id,
                "length": version.len.to_string(),
                "sha256": version.sha256,
                "encoding": version.encoding,
                "acquisition": version.acquisition,
            })),
            Err(error) => Some(serde_json::json!({
                "status": "incomplete",
                "version_id": version_id,
                "length": null,
                "sha256": null,
                "encoding": null,
                "acquisition": null,
                "error": format!("version record missing ({version_id}): {error}"),
            })),
        },
        None => None,
    };
    Ok(serde_json::json!({
        "store": local_scope(store),
        "object_kind": object_kind,
        "node": object,
        "chain_root_commit_id": object["root_commit_id"],
        "tip_commit_id": tip,
        "selected_commit_id": selected,
        "effective_range_commit_id": if reference_kind == "range" { serde_json::to_value(effective_commit).unwrap() } else { serde_json::Value::Null },
        "source_version_id": source_version.as_ref().map(|version| version["version_id"].clone()),
        "source_version": source_version,
        "owning_file": owning_file,
        "project_relative_path": path,
        "position": position.map(omd::relations::node::position_value),
    }))
}

fn commit_projection(
    store: &Store,
    commit_id: &str,
    current_location: bool,
) -> Result<serde_json::Value, AppError> {
    let resolved = omd::relations::identity::resolve_commit_id(store, commit_id)
        .map_err(|error| AppError::usage(error.to_string()))?;
    let (node, _) = omd::relations::identity::commit_to_node(store, &resolved)
        .map_err(|error| AppError::usage(error.to_string()))?;
    object_projection(store, &node, &resolved, current_location)
}

fn endpoint_projection(store: &Store, node: &str, selected: &str) -> serde_json::Value {
    serde_json::json!({
        "object": object_ref_value(store, node),
        "selected_version_commit_id": selected,
    })
}

fn link_projection(store: &Store, link: &omd::records::store::Link) -> serde_json::Value {
    serde_json::json!({
        "link_id": link.link_id,
        "creation_commit_id": link.created_by,
        "source": endpoint_projection(store, &link.source, &link.source_version),
        "target": endpoint_projection(store, &link.target, &link.target_version),
    })
}

fn current_objects(store: &Store) -> Result<Vec<serde_json::Value>, AppError> {
    store
        .state()
        .tips
        .iter()
        .map(|(node, tip)| object_projection(store, node, tip, true))
        .collect()
}

fn current_links(store: &Store) -> Vec<serde_json::Value> {
    store
        .state()
        .links
        .values()
        .map(|link| link_projection(store, link))
        .collect()
}

fn successful_member(kind: &str, id: impl Into<String>) -> serde_json::Value {
    serde_json::json!({ "kind": kind, "id": id.into() })
}

fn verification_issues(store: &Store, report: &pipeline::VerifyReport) -> Vec<serde_json::Value> {
    let mut issues = Vec::new();
    for (node, entries) in &report.dirty {
        for entry in entries {
            let persisted = store
                .state()
                .dirty
                .get(node)
                .and_then(|state| state.dirty.get(entry));
            let (commit_id, message) = match persisted {
                Some(omd::relations::dirty::DirtyReason::ContentChanged) => {
                    (entry.clone(), "tracked content changed".to_string())
                }
                Some(omd::relations::dirty::DirtyReason::EndAdjacentInsertion) => (
                    entry.clone(),
                    "content was inserted at the tracked range end".to_string(),
                ),
                Some(omd::relations::dirty::DirtyReason::DependencyDangling { dependency }) => (
                    entry.clone(),
                    format!("dependency is dangling: {dependency}"),
                ),
                Some(omd::relations::dirty::DirtyReason::ExplicitUnclean { reason }) => {
                    (entry.clone(), reason.clone())
                }
                None => (
                    store.state().tips.get(node).cloned().unwrap_or_default(),
                    entry.clone(),
                ),
            };
            issues.push(serde_json::json!({
                "kind": "dirty",
                "severity": "error",
                "message": message,
                "store": store.state().store_id,
                "node": object_ref_value(store, node),
                "commit_id": commit_id,
            }));
        }
    }
    issues
}

fn collect_projection_diagnostics(value: &serde_json::Value, diagnostics: &mut Vec<Diagnostic>) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                collect_projection_diagnostics(value, diagnostics);
            }
        }
        serde_json::Value::Object(object) => {
            if let Some(version) = object.get("source_version")
                && version.get("status").and_then(|value| value.as_str()) == Some("incomplete")
            {
                let message = version
                    .get("error")
                    .and_then(|value| value.as_str())
                    .unwrap_or("source version unavailable");
                let store = object
                    .get("store")
                    .and_then(|value| value.get("store_id"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                diagnostics.push(
                    Diagnostic::new("incomplete", "error", message).context(
                        store,
                        object.get("node").cloned(),
                        object
                            .get("selected_commit_id")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                    ),
                );
            }
            for value in object.values() {
                collect_projection_diagnostics(value, diagnostics);
            }
        }
        _ => {}
    }
}

fn diagnostics_from_data(data: &serde_json::Value) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let report = data.get("check").unwrap_or(data);
    if let Some(issues) = report
        .get("diagnostic_issues")
        .and_then(serde_json::Value::as_array)
    {
        diagnostics.extend(issues.iter().filter_map(|issue| {
            Some(
                Diagnostic::new(
                    issue.get("kind")?.as_str()?,
                    issue.get("severity")?.as_str()?,
                    issue.get("message")?.as_str()?,
                )
                .context(
                    issue
                        .get("store")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    issue.get("node").filter(|value| !value.is_null()).cloned(),
                    issue
                        .get("commit_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ),
            )
        }));
    }
    for (field, kind) in [
        ("identity", "identity"),
        ("unverified", "incomplete"),
        ("missing", "missing_source"),
        ("problems", "scope_problem"),
    ] {
        if let Some(values) = report.get(field).and_then(serde_json::Value::as_array) {
            diagnostics.extend(
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(|message| Diagnostic::new(kind, "error", message)),
            );
        }
    }
    if let Some(rules) = report.get("rules").and_then(serde_json::Value::as_array) {
        for rule in rules {
            let name = rule
                .get("rule")
                .and_then(|value| value.as_str())
                .unwrap_or("rule");
            let status = rule
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("incomplete");
            let severity = if rule.get("level").and_then(|value| value.as_str()) == Some("warn") {
                "warning"
            } else {
                "error"
            };
            if matches!(status, "fail" | "incomplete") {
                diagnostics.push(Diagnostic::new(
                    if status == "fail" {
                        "check_failed"
                    } else {
                        "incomplete"
                    },
                    severity,
                    format!("{name}: {status}"),
                ));
            }
            for direction in ["forward", "reverse"] {
                let Some(files) = rule
                    .get("coverage")
                    .and_then(|coverage| coverage.get(direction))
                    .and_then(|coverage| coverage.get("files"))
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for file in files.iter().filter(|file| file["status"] == "incomplete") {
                    let message = file
                        .get("reason")
                        .and_then(|value| value.as_str())
                        .unwrap_or("coverage source unavailable");
                    let store = file
                        .get("store")
                        .and_then(|value| value.get("store_id"))
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    let commit_id = file
                        .get("commit_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    diagnostics.push(Diagnostic::new("incomplete", severity, message).context(
                        store,
                        file.get("node").filter(|value| !value.is_null()).cloned(),
                        commit_id,
                    ));
                }
            }
        }
    }
    collect_projection_diagnostics(data, &mut diagnostics);
    diagnostics
}

fn emit(cli: &Cli, v: serde_json::Value) {
    if cli.json {
        println!("{}", serde_json::to_string(&v).unwrap());
    } else {
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    }
}

fn run_project_action(
    action: &ProjectAction,
    expected: Option<&Expected>,
) -> Result<serde_json::Value, AppError> {
    let cwd = std::env::current_dir()
        .map_err(|error| AppError::io(format!("current directory: {error}")))?;
    match action {
        ProjectAction::Register {
            alias,
            project_root,
            metadata_dir,
            git_remote,
            git_remote_url,
        } => {
            let expected = expected.ok_or_else(|| {
                AppError::usage("project mutation requires --expected <JSON_OR_FILE>")
            })?;
            let remote = match (git_remote, git_remote_url) {
                (Some(name), Some(url)) => Some(omd::records::registration::RemoteIdentity {
                    name: name.clone(),
                    url: url.clone(),
                }),
                (None, None) => None,
                _ => {
                    return Err(AppError::usage(
                        "--git-remote and --git-remote-url are required together",
                    ));
                }
            };
            let mapping = omd::sources::projects::register(
                &cwd,
                alias,
                PathBuf::from(project_root),
                PathBuf::from(metadata_dir),
                remote,
                expected,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "alias": mapping.alias,
                "project_id": mapping.project_id,
                "store_id": mapping.store_id,
                "project_root": mapping.project_root,
                "metadata_root": mapping.metadata_root,
            }))
        }
        ProjectAction::RecognizeRemote {
            alias,
            git_remote_url,
        } => {
            let expected = expected.ok_or_else(|| {
                AppError::usage("project mutation requires --expected <JSON_OR_FILE>")
            })?;
            omd::sources::projects::recognize_remote(&cwd, alias, git_remote_url, expected)?;
            Ok(serde_json::json!({
                "ok": true,
                "alias": alias,
                "recognized_remote_url": git_remote_url,
            }))
        }
        ProjectAction::List => {
            let mappings = omd::sources::projects::load(&cwd)?;
            for mapping in &mappings {
                let identity = Store::identity_at(&mapping.metadata_root).map_err(|error| {
                    AppError::usage(format!(
                        "project mapping {} is invalid: {error}",
                        mapping.alias
                    ))
                })?;
                let registration = Store::open_existing(&mapping.metadata_root)
                    .and_then(|store| store.project_registration(&mapping.alias))
                    .map_err(|error| {
                        AppError::usage(format!(
                            "project registration {} is invalid: {error}",
                            mapping.alias
                        ))
                    })?;
                if identity.project_id != mapping.project_id
                    || identity.store_id != mapping.store_id
                    || registration.project_id != mapping.project_id
                    || registration.store_id != mapping.store_id
                {
                    return Err(AppError::usage(format!(
                        "project mapping {} has wrong project/store identity",
                        mapping.alias
                    )));
                }
            }
            let mut rows = Vec::new();
            for mapping in mappings {
                let registration = Store::open_existing(&mapping.metadata_root)?
                    .project_registration(&mapping.alias)?;
                rows.push(serde_json::json!({
                    "alias": mapping.alias,
                    "project_id": mapping.project_id,
                    "store_id": mapping.store_id,
                    "project_root": mapping.project_root,
                    "metadata_root": mapping.metadata_root,
                    "revision": mapping.revision,
                    "recognized_remote_urls": mapping.recognized_remote_urls,
                    "registration": registration,
                }));
            }
            Ok(serde_json::json!({ "ok": true, "projects": rows }))
        }
    }
}

fn collect_new_source(
    cli: &Cli,
    context: &omd::sources::projects::ProjectContext,
    target_path: &str,
    byte_mode: bool,
) -> Result<
    (
        omd::sources::SourceDescriptor,
        omd::sources::SourceDescriptor,
        omd::sources::Observation,
    ),
    AppError,
> {
    let cwd = std::env::current_dir()
        .map_err(|error| AppError::io(format!("current directory: {error}")))?;
    let selected = source_fields(cli)
        .descriptor(Some(target_path), false)
        .map_err(|error| AppError::usage(error.to_string()))?
        .expect("new source has a default descriptor");
    let selected = omd::sources::normalize_descriptor(selected, &cwd, &context.project_root)?;
    let (observation, recovery) = match selected {
        omd::sources::SourceDescriptor::File { .. }
        | omd::sources::SourceDescriptor::Git { .. } => (
            omd::sources::SourceDescriptor::File {
                project: "root".into(),
                path: target_path.to_string(),
            },
            selected,
        ),
        other => (other.clone(), other),
    };
    let encoding = if byte_mode {
        None
    } else {
        Some(omd::sources::encoding::resolve_runtime(
            cli.encoding.as_deref(),
            None,
            &cwd,
            &context.metadata_root,
            target_path,
        )?)
    };
    let collected = omd::sources::collect(
        &recovery,
        &cwd,
        &context.project_root,
        !byte_mode,
        encoding.as_deref(),
    )?;
    Ok((observation, recovery, collected))
}

fn existing_source(
    cli: &Cli,
    store: &Store,
    node: &str,
    target_path: &str,
) -> Result<omd::sources::SourceDescriptor, AppError> {
    let version_id = store
        .source_version_id(node)?
        .ok_or_else(|| AppError::usage(format!("source version missing for {node}")))?;
    let version = store.read_version(&version_id)?;
    let current = store
        .current_acquisition(node, &version)
        .ok_or_else(|| AppError::usage(format!("current source missing for {node}")))?;
    let fields = source_fields(cli);
    if fields.any() {
        let supplied = fields
            .descriptor(Some(target_path), false)
            .map_err(|error| AppError::usage(error.to_string()))?
            .expect("supplied source fields produce a descriptor");
        let cwd = std::env::current_dir()
            .map_err(|error| AppError::io(format!("current directory: {error}")))?;
        let supplied = omd::sources::normalize_descriptor(
            supplied,
            &cwd,
            store.project_root().ok_or_else(|| {
                AppError::usage("source validation requires selected project root")
            })?,
        )?;
        if supplied != current {
            return Err(AppError::usage(format!(
                "source fields do not match the registered {} observation definition",
                current.kind()
            )));
        }
    }
    Ok(current)
}

fn run(cli: &Cli) -> Result<serde_json::Value, AppError> {
    preflight_source_fields(cli)?;
    let supplied_expected = cli
        .expected
        .as_ref()
        .map(|_| expected_from_cli(cli))
        .transpose()?;
    if let Cmd::Project { action } = &cli.cmd {
        return run_project_action(action, supplied_expected.as_ref());
    }
    let context = project_context(cli)?;
    let expected = supplied_expected.unwrap_or_default();
    let root = context.metadata_root.clone();
    let project_root = context.project_root.clone();
    match &cli.cmd {
        Cmd::Delete { path } => {
            let (path, _) = omd::sources::projects::project_path(&project_root, path)?;
            let mut store = open_context_store(&context)?;
            let cid = pipeline::commit_lifecycle(
                &mut store,
                &mut NoProbe,
                &OsRng,
                &SystemClock,
                CommitKind::Delete,
                &path,
                None,
                "",
                &expected,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "commit": cid,
                "kind": "Delete",
                "object": commit_projection(&store, &cid, true)?,
            }))
        }
        Cmd::Init { path } | Cmd::Import { path, .. } | Cmd::Remove { path } => {
            let kind = match &cli.cmd {
                Cmd::Init { .. } => CommitKind::Init,
                Cmd::Import { .. } => CommitKind::Import,
                Cmd::Remove { .. } => CommitKind::Remove,
                _ => unreachable!(),
            };
            let (path, current_path) = omd::sources::projects::project_path(&project_root, path)?;
            let initializing = kind == CommitKind::Init
                && matches!(
                    &context.metadata,
                    omd::sources::projects::MetadataSelection::Initialize
                        | omd::sources::projects::MetadataSelection::InitializeConfigured
                );
            let prepared_before_open = if initializing {
                Some(collect_new_source(cli, &context, &path, false)?)
            } else {
                None
            };
            let mut store = if initializing {
                let store = open_initial_store(&context)?;
                let cwd = std::env::current_dir()
                    .map_err(|error| AppError::io(format!("current directory: {error}")))?;
                omd::sources::projects::authorize_instance(
                    &cwd,
                    &project_root,
                    &root,
                    &store.identity(),
                )?;
                store
            } else {
                open_context_store(&context)?
            };
            let _ = current_path;
            let existing = store
                .object_at_path(&path, kind != CommitKind::Init)
                .map(str::to_string);
            if matches!(&cli.cmd, Cmd::Init { .. }) && existing.is_some() {
                return Err(AppError::usage(format!(
                    "init refused: {path} is already tracked (init is not stackable)"
                )));
            }
            if matches!(&cli.cmd, Cmd::Remove { .. }) && existing.is_none() {
                return Err(AppError::usage(format!(
                    "remove refused: {path} is not tracked"
                )));
            }
            let node = existing.unwrap_or_else(|| "file:pending".into());
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            if let Cmd::Import {
                exclude, include, ..
            } = &cli.cmd
            {
                payload.insert("exclude".into(), exclude.clone().into());
                payload.insert("include".into(), include.clone().into());
                // Record the resolved scope so statistics can re-resolve it:
                // excludes applied first, includes override (gitignore !-cascade).
                let mut stream: Vec<String> = exclude.clone();
                stream.extend(include.iter().map(|i| format!("!{i}")));
                payload.insert("scope".into(), stream.into());
            }
            // Import/remove records statistics only, for files and directories. Source init
            // uses the closed descriptor and one pre-collected complete value.
            let cid = if kind == CommitKind::Init {
                let (acquisition, recovery, collected) = match prepared_before_open {
                    Some(prepared) => prepared,
                    None => collect_new_source(cli, &context, &path, false)?,
                };
                pipeline::commit_source(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    &SystemClock,
                    &node,
                    acquisition,
                    recovery,
                    Some(collected),
                    cli.encoding.as_deref(),
                    kind,
                    payload,
                    &expected,
                )?
            } else {
                pipeline::commit_marker(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    &SystemClock,
                    &node,
                    kind,
                    payload,
                    &expected,
                )?
            };
            Ok(serde_json::json!({
                "ok": true,
                "commit": cid,
                "kind": format!("{kind:?}"),
                "object": commit_projection(&store, &cid, true)?,
            }))
        }
        Cmd::Commit {
            kind,
            path,
            reason,
            range,
            mode,
            timestamp,
            reset_target,
            source,
            target,
            source_store,
            target_store,
            stop,
            adapt,
            no_reason,
            link_from,
            link_to,
            link_from_store,
            link_to_store,
            id,
            tag,
            rule,
            level,
            skip,
            expect_version,
        } => {
            let kind = match kind.as_str() {
                "init" => CommitKind::Init,
                "commit" => CommitKind::Commit,
                "clean" => CommitKind::Clean,
                "unclean" => CommitKind::Unclean,
                "import" => CommitKind::Import,
                "remove" => CommitKind::Remove,
                "delete" => CommitKind::Delete,
                "verify" | "file_verify" => CommitKind::FileVerify,
                "reset" => CommitKind::Reset,
                "atomic_begin" | "begin" => CommitKind::AtomicBegin,
                "atomic_end" | "end" => CommitKind::AtomicEnd,
                "link" => CommitKind::Link,
                "adapt" => CommitKind::Adapt,
                "tag" => CommitKind::Tag,
                "scope_adjust" => CommitKind::ScopeAdjust,
                other => return Err(AppError::usage(format!("unknown commit kind: {other}"))),
            };
            let (logical_path, _current_path) =
                omd::sources::projects::project_path(&project_root, path)?;
            let path = &logical_path;
            let initializing = kind == CommitKind::Init
                && matches!(
                    &context.metadata,
                    omd::sources::projects::MetadataSelection::Initialize
                        | omd::sources::projects::MetadataSelection::InitializeConfigured
                );
            if initializing && !(range.is_some() && mode.as_deref() == Some("byte")) {
                let cwd = std::env::current_dir()
                    .map_err(|error| AppError::io(format!("current directory: {error}")))?;
                omd::sources::encoding::resolve_runtime(
                    cli.encoding.as_deref(),
                    None,
                    &cwd,
                    &context.metadata_root,
                    path,
                )?;
            }
            let mut store = if initializing {
                let store = open_initial_store(&context)?;
                let cwd = std::env::current_dir()
                    .map_err(|error| AppError::io(format!("current directory: {error}")))?;
                omd::sources::projects::authorize_instance(
                    &cwd,
                    &project_root,
                    &root,
                    &store.identity(),
                )?;
                store
            } else {
                open_context_store(&context)?
            };
            if kind != CommitKind::Init {
                store.require_identity()?;
            }
            // An unactivated copy (store dir copied, not re-registered) must
            // not publish business writes — history/diagnostic reads only.
            if !omd::records::cross::activated(&store) {
                return Err("store is an unregistered copy: business writes refused until `omd activate` registers a new store_id".into());
            }
            let explicit_link_endpoints = if kind == CommitKind::Link {
                let source_commit = source
                    .clone()
                    .ok_or_else(|| AppError::usage("commit link requires --source"))?;
                let target_commit = target
                    .clone()
                    .ok_or_else(|| AppError::usage("commit link requires --target"))?;
                let source_arg = match source_store {
                    Some(alias) => EndpointArg::Store {
                        direction: LinkDirection::From,
                        alias: alias.clone(),
                        commit: source_commit,
                    },
                    None => EndpointArg::Local {
                        direction: LinkDirection::From,
                        commit: source_commit,
                    },
                };
                let target_arg = match target_store {
                    Some(alias) => EndpointArg::Store {
                        direction: LinkDirection::To,
                        alias: alias.clone(),
                        commit: target_commit,
                    },
                    None => EndpointArg::Local {
                        direction: LinkDirection::To,
                        commit: target_commit,
                    },
                };
                Some((
                    resolve_endpoint_arg(&store, source_arg, &expected)?,
                    resolve_endpoint_arg(&store, target_arg, &expected)?,
                ))
            } else {
                if source.is_some()
                    || target.is_some()
                    || source_store.is_some()
                    || target_store.is_some()
                {
                    return Err(AppError::usage(
                        "--source/--target and endpoint store fields require `commit link`",
                    ));
                }
                None
            };

            // Resolve every directional endpoint before BEGIN and normalize
            // duplicates by direction + actual store namespace + chain root.
            let expected_endpoint_count = link_from.len()
                + link_to.len()
                + link_from_store.len() / 2
                + link_to_store.len() / 2;
            let endpoint_args = ordered_endpoint_args()?;
            if endpoint_args.len() != expected_endpoint_count {
                return Err(AppError::usage(
                    "endpoint options must use separate option/value arguments",
                ));
            }
            let mut resolved_endpoints = Vec::with_capacity(endpoint_args.len());
            let mut seen = std::collections::HashSet::new();
            for endpoint in endpoint_args {
                let resolved = resolve_endpoint_arg(&store, endpoint, &expected)?;
                if !seen.insert((
                    resolved.direction,
                    resolved.actual_store_id.clone(),
                    resolved.root.clone(),
                )) {
                    let direction = match resolved.direction {
                        LinkDirection::From => "--link-from",
                        LinkDirection::To => "--link-to",
                    };
                    return Err(AppError::usage(format!(
                        "duplicate {direction} endpoint in one command: {}",
                        resolved.label
                    )));
                }
                resolved_endpoints.push(resolved);
            }

            let mut locked_peers = std::collections::BTreeMap::<String, Store>::new();
            let peer_ids = resolved_endpoints
                .iter()
                .filter_map(|endpoint| endpoint.peer_store_id.as_ref())
                .chain(
                    explicit_link_endpoints
                        .iter()
                        .flat_map(|(source, target)| [source, target])
                        .filter_map(|endpoint| endpoint.peer_store_id.as_ref()),
                )
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            for peer_store_id in peer_ids {
                locked_peers.insert(
                    peer_store_id.clone(),
                    peer_store(&store, &peer_store_id, &expected)?,
                );
            }
            if kind != CommitKind::Init {
                lock_participants(&mut store, &mut locked_peers)?;
                store.require_identity()?;
                store.check_expected(&expected)?;
                for (peer_store_id, peer) in &mut locked_peers {
                    let observed = expected.peers.get(peer_store_id).ok_or_else(|| {
                        AppError::version(format!(
                            "caller has no successful peer observation for {peer_store_id}"
                        ))
                    })?;
                    validate_peer_locked(&store, peer, observed)?;
                }
            }
            // Resolve the node: --range creates/targets a first-class range
            // chain mounted under the file; bare path targets the file chain.
            // A range node's key is `range:<chain-root-commit-id>` — identity
            // is the chain, never the coordinates.
            if matches!(kind, CommitKind::Tag | CommitKind::ScopeAdjust)
                && id.is_none()
                && store.file_at_path(path).is_some()
                && store.object_at_path(path, true).is_some()
            {
                return Err(AppError::usage(
                    "ambiguous content/import path: select the current object tip with --id",
                ));
            }
            let existing_file = if matches!(kind, CommitKind::Import | CommitKind::Remove) {
                store.object_at_path(path, true)
            } else {
                store.file_at_path(path).or_else(|| {
                    matches!(kind, CommitKind::Tag | CommitKind::ScopeAdjust)
                        .then(|| store.object_at_path(path, true))
                        .flatten()
                })
            }
            .map(str::to_string);
            let file_node = match (kind, existing_file) {
                (CommitKind::Init | CommitKind::Import, None) => "file:pending".into(),
                (CommitKind::Init, Some(_)) => {
                    return Err(AppError::usage(format!(
                        "init refused: {path} is already tracked"
                    )));
                }
                (_, Some(node)) => node,
                (_, None) => {
                    return Err(AppError::usage(format!("file is not tracked: {path}")));
                }
            };
            let node = match (range, id) {
                // --id names a commit in the target chain — resolve it to the
                // node whose chain contains that commit.
                (_, Some(cid)) => match omd::relations::identity::commit_to_node(&store, cid) {
                    Ok((key, _)) => {
                        // --id names a commit on the target chain and
                        // asserts it IS the chain's current tip — a stale
                        // commit id is a version conflict, never a silent
                        // rebase onto the newest tip (spec: "不自动改用
                        // r1"). Resolve the supplied id to its exact
                        // commit on this chain first (prefix forms too).
                        let sel =
                            omd::relations::identity::resolve_version_on_chain(&store, &key, cid)
                                .map_err(|e| AppError::usage(format!("--id: {e}")))?;
                        let tip = store.state().tips.get(&key).cloned().unwrap_or_default();
                        if sel != tip {
                            return Err(AppError::version(format!(
                                "version conflict: --id {sel} is not the current tip {tip}"
                            )));
                        }
                        if range.is_some() && !key.starts_with("range:") {
                            return Err(AppError::usage(format!(
                                "--range with --id requires a range commit, got {key}"
                            )));
                        }
                        if key.starts_with("range:") {
                            let parent = store
                                .state()
                                .mounts
                                .iter()
                                .find_map(|(parent, children)| {
                                    children.iter().any(|child| child == &key).then_some(parent)
                                })
                                .ok_or_else(|| {
                                    AppError::new(
                                        ErrorKind::Check,
                                        format!("range has no mounted file: {key}"),
                                    )
                                })?;
                            if parent != &file_node {
                                return Err(AppError::usage(format!(
                                    "--id range belongs to {parent}, not {file_node}"
                                )));
                            }
                        }
                        key
                    }
                    Err(error) => {
                        return Err(AppError::usage(format!("--id: {error}")));
                    }
                },
                // New range over the file: the node's key is its chain-root
                // commit id — `range:<cid>`. For a combo the ATOMIC BEGIN is
                // the root; for a bare commit the commit itself is. The id is
                // only known once the first commit publishes, so this arm
                // emits a pending placeholder that resolves to `range:<cid>`
                // right after the first commit lands (see below).
                (Some(_), None) => {
                    if let Some(other) = mode.as_deref()
                        && !matches!(other, "byte" | "text")
                    {
                        return Err(AppError::usage(format!("bad --mode: {other}")));
                    }
                    String::from("range:pending")
                }
                (None, None) => file_node.clone(),
            };
            // Resolve and validate range position before any BEGIN/member
            // publication. Continuations without --mode inherit the effective
            // range version; new ranges default to text.
            let requested_range = if let Some(raw) = range {
                let [start, end] = raw.as_slice() else {
                    return Err(AppError::usage("--range requires START END"));
                };
                let inherited =
                    if omd::relations::node::is_range_key(&node) && node != "range:pending" {
                        omd::relations::identity::effective_range_state(&store, &node)
                            .ok()
                            .and_then(|state| state.range.map(|range| range.mode))
                    } else {
                        None
                    };
                let unit = match mode.as_deref() {
                    Some("byte") => omd::relations::range::Mode::Byte,
                    Some("text") => omd::relations::range::Mode::Text,
                    Some(other) => {
                        return Err(AppError::usage(format!("bad --mode: {other}")));
                    }
                    None => inherited.unwrap_or(omd::relations::range::Mode::Text),
                };
                let evidence_node = if node == "range:pending" {
                    &file_node
                } else {
                    &node
                };
                let descriptor = existing_source(cli, &store, evidence_node, path)?;
                let bytes = if let omd::sources::SourceDescriptor::File { .. } = descriptor {
                    Some(
                        std::fs::read(store.resolve_file_descriptor(&descriptor)?).map_err(
                            |e| {
                                AppError::io(format!("cannot observe {path} for range bounds: {e}"))
                            },
                        )?,
                    )
                } else {
                    None
                };
                Some(if let Some(bytes) = bytes {
                    let len = match unit {
                        omd::relations::range::Mode::Byte => bytes.len() as u64,
                        omd::relations::range::Mode::Text => {
                            let recorded = store
                                .source_version_id(evidence_node)?
                                .and_then(|version| store.read_version(&version).ok())
                                .and_then(|version| version.encoding);
                            let cwd = store.config_cwd().ok_or_else(|| {
                                AppError::usage("source view requires local configuration")
                            })?;
                            let encoding = omd::sources::encoding::resolve_runtime(
                                cli.encoding.as_deref(),
                                recorded.as_deref(),
                                cwd,
                                &root,
                                path,
                            )?;
                            omd::relations::range::text_len(
                                &decode_range_text(&bytes, &encoding).ok_or_else(|| {
                                    AppError::io(format!("cannot decode {path} as {encoding}"))
                                })?,
                            )
                        }
                    };
                    omd::relations::range::Range::new(*start, *end, unit, len)
                        .map_err(|e| AppError::usage(format!("bad --range: {e}")))?
                } else {
                    omd::relations::range::Range {
                        start: *start,
                        end: *end,
                        mode: unit,
                    }
                })
            } else {
                None
            };
            // When creating a new range, declare its mount parent (the file
            // node) in the payload — the range key carries no location, so
            // the mount relationship must be explicit on the first commit.
            let pending_range = node == "range:pending";
            let combo = !resolved_endpoints.is_empty();
            if combo && !pending_range && !omd::relations::node::is_range_key(&node) {
                return Err(AppError::usage(
                    "link options require a range target; provide --range or a range --id",
                ));
            }
            // --expect-version: optimistic-concurrency guard on the node's
            // CURRENT effective tip (for a range, the parent-snapshot tip;
            // for a file, its own tip). A mismatch is a version conflict —
            // never a silent overwrite of a chain the caller didn't see.
            if let Some(ev) = expect_version {
                let cur_tip = store.state().tips.get(&node).cloned().unwrap_or_default();
                if *ev != cur_tip {
                    return Err(AppError::version(format!(
                        "version conflict: expected {ev}, current tip {cur_tip}"
                    )));
                }
            }
            let mut reset_outcome: Option<serde_json::Value> = None;
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            if pending_range {
                // The new range mounts under its file node.
                payload.insert("mount".into(), file_node.clone().into());
            }
            if let Some(r) = reason {
                payload.insert("reason".into(), r.clone().into());
            }
            if let Some(r) = requested_range {
                payload.insert("position".into(), omd::relations::node::position_value(r));
            }
            if *no_reason {
                payload.insert("no_reason".into(), true.into());
            }
            if let Some(t) = tag {
                payload.insert("tag".into(), t.clone().into());
            }
            if let Some(r) = rule {
                payload.insert("rule".into(), r.clone().into());
            }
            if let Some(l) = level {
                payload.insert("level".into(), l.clone().into());
            }
            if *skip {
                payload.insert("skip".into(), true.into());
            }
            // --timestamp: manual replay — pin the clock to the user's
            // recorded time so the commit's canonical timestamp (and thus
            // its id) reflects it. Used to order commits for conflict replay.
            let fixed = timestamp
                .as_deref()
                .map(|t| {
                    omd::records::time::parse_rfc3339(t)
                        .ok_or_else(|| AppError::usage("bad --timestamp: need RFC3339"))
                })
                .transpose()?;
            let fixed_clock = fixed.map(omd::records::time::FixedClock);
            let clock: &dyn omd::testing::Clock = fixed_clock
                .as_ref()
                .map(|c| c as &dyn omd::testing::Clock)
                .unwrap_or(&SystemClock);
            // State-only kinds observe no file; content kinds read the file.
            let state_only = matches!(
                kind,
                CommitKind::Unclean
                    | CommitKind::Clean
                    | CommitKind::AtomicBegin
                    | CommitKind::AtomicEnd
                    | CommitKind::Reset
                    | CommitKind::ScopeAdjust
                    | CommitKind::Import
                    | CommitKind::Remove
                    | CommitKind::Tag
            );
            if state_only && source_fields(cli).any() {
                return Err(AppError::usage(
                    "state-only commit does not accept source fields",
                ));
            }
            // --link-from/--link-to wrap the commit in an ATOMIC block: the
            // range commit + each link creation are recorded inside one
            // BEGIN/END on this node's chain (spec 8.4 atomic combination).
            // For a NEW range the BEGIN is the chain root — the node key is
            // `range:<begin-commit-id>`; begin commits first, then we rekey.
            // Every local or cross-store link leg belongs to one sequential
            // ATOMIC publication. Inputs were fully prepared above; failures
            // after BEGIN report exactly what already landed.
            let operation_id = if combo {
                let mut idb = [0u8; 16];
                omd::testing::Rng::fill(&OsRng, &mut idb);
                hex::encode(idb)
            } else {
                String::new()
            };
            let mut succeeded_members: Vec<serde_json::Value> = Vec::new();
            let mut node = node; // rekeyed below for pending ranges
            if combo && !state_only {
                let evidence_node = if pending_range { &file_node } else { &node };
                let _descriptor = existing_source(cli, &store, evidence_node, path)?;
                store.lock()?;
                store.require_identity()?;
                store.check_expected(&expected)?;
                store.require_source_expected(&expected, evidence_node)?;
            }
            if combo {
                let mut pl = serde_json::Map::new();
                pl.insert("path".into(), path.clone().into());
                if pending_range {
                    // BEGIN is the chain root: it mounts the pending range
                    // under its file node. After publish we rekey the node
                    // to `range:<begin-cid>` so the BEGIN's own id is the key.
                    pl.insert("mount".into(), file_node.clone().into());
                }
                let begin_cid = pipeline::commit_marker(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    clock,
                    &node,
                    CommitKind::AtomicBegin,
                    pl,
                    &expected,
                )?;
                succeeded_members.push(successful_member("commit", begin_cid.clone()));
                if pending_range {
                    node = format!("range:{begin_cid}");
                }
            }
            // The business commit lands on the (possibly rekeyed) node.
            let cid_result: Result<String, AppError> = (|| {
                let cid = if kind == CommitKind::Link {
                    let (source_endpoint, target_endpoint) = explicit_link_endpoints
                        .as_ref()
                        .ok_or_else(|| AppError::usage("commit link requires endpoints"))?;
                    let r = reason.clone().unwrap_or_default();
                    match (
                        source_endpoint.peer_store_id.as_ref(),
                        target_endpoint.peer_store_id.as_ref(),
                    ) {
                        (None, None) => pipeline::commit_link(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &source_endpoint.root,
                            &source_endpoint.version,
                            &target_endpoint.root,
                            &target_endpoint.version,
                            &r,
                            &expected,
                        )?,
                        (Some(peer_store_id), None) => {
                            let peer = locked_peers.get_mut(peer_store_id).ok_or_else(|| {
                                AppError::version(format!(
                                    "locked peer participant is missing: {peer_store_id}"
                                ))
                            })?;
                            let observed = expected.peers.get(peer_store_id).ok_or_else(|| {
                                AppError::version(format!(
                                    "caller has no observation for peer {peer_store_id}"
                                ))
                            })?;
                            let mut id = [0u8; 16];
                            omd::testing::Rng::fill(&OsRng, &mut id);
                            pipeline::commit_external_link_protected(
                                &mut store,
                                peer,
                                &mut NoProbe,
                                &OsRng,
                                clock,
                                &node,
                                &target_endpoint.root,
                                &target_endpoint.version,
                                peer_store_id,
                                &source_endpoint.root,
                                &source_endpoint.version,
                                &hex::encode(id),
                                &r,
                                &expected,
                                observed,
                                pipeline::ExternalLinkDirection::PeerToLocal,
                            )
                            .map_err(|failure| failure.error)?
                            .link_id
                        }
                        (None, Some(peer_store_id)) => {
                            let peer = locked_peers.get_mut(peer_store_id).ok_or_else(|| {
                                AppError::version(format!(
                                    "locked peer participant is missing: {peer_store_id}"
                                ))
                            })?;
                            let observed = expected.peers.get(peer_store_id).ok_or_else(|| {
                                AppError::version(format!(
                                    "caller has no observation for peer {peer_store_id}"
                                ))
                            })?;
                            let mut id = [0u8; 16];
                            omd::testing::Rng::fill(&OsRng, &mut id);
                            pipeline::commit_external_link_protected(
                                &mut store,
                                peer,
                                &mut NoProbe,
                                &OsRng,
                                clock,
                                &node,
                                &source_endpoint.root,
                                &source_endpoint.version,
                                peer_store_id,
                                &target_endpoint.root,
                                &target_endpoint.version,
                                &hex::encode(id),
                                &r,
                                &expected,
                                observed,
                                pipeline::ExternalLinkDirection::LocalToPeer,
                            )
                            .map_err(|failure| failure.error)?
                            .link_id
                        }
                        (Some(_), Some(_)) => {
                            return Err(AppError::usage(
                                "commit link requires at least one endpoint in the selected local store",
                            ));
                        }
                    }
                } else if kind == CommitKind::Adapt {
                    if adapt.is_empty() {
                        return Err(AppError::usage(
                            "commit adapt requires one or more --adapt JSON objects",
                        ));
                    }
                    let mut last = String::new();
                    for entry in adapt {
                        let selection: AdaptArg = serde_json::from_str(entry).map_err(|error| {
                            AppError::usage(format!("bad --adapt JSON: {error}"))
                        })?;
                        last = pipeline::commit_adapt(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &selection.link_id,
                            &selection.changes,
                            &selection.reason,
                            false,
                            false,
                            &expected,
                        )?;
                    }
                    last
                } else if kind == CommitKind::Clean {
                    if stop.is_empty() {
                        return Err(AppError::usage(
                            "commit clean requires one or more --stop JSON objects",
                        ));
                    }
                    if reason.is_some() == *no_reason {
                        return Err(AppError::usage(
                            "commit clean requires exactly one of --reason or --no--reason",
                        ));
                    }
                    let mut last = String::new();
                    for entry in stop {
                        let selection: StopArg = serde_json::from_str(entry).map_err(|error| {
                            AppError::usage(format!("bad --stop JSON: {error}"))
                        })?;
                        last = pipeline::commit_adapt(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &selection.link_id,
                            &selection.changes,
                            reason.as_deref().unwrap_or(""),
                            true,
                            *no_reason,
                            &expected,
                        )?;
                    }
                    last
                } else if kind == CommitKind::Reset {
                    // reset resolves the target's kind+prev from its on-disk record.
                    // The target is an explicit --target <commit-id> — never a
                    // reason overload. `--reason` on reset is a usage error.
                    if reason.is_some() {
                        return Err(AppError::usage(
                            "reset takes --reset-target <commit-id>, not --reason",
                        ));
                    }
                    let target = reset_target.clone().ok_or_else(|| {
                        AppError::usage("reset requires --reset-target <commit-id>")
                    })?;
                    let target = omd::relations::identity::resolve_commit_id(&store, &target)
                        .map_err(|e| AppError::usage(format!("--reset-target: {e}")))?;
                    let (reset_node, _) = omd::relations::identity::commit_to_node(&store, &target)
                        .map_err(|e| AppError::usage(format!("--reset-target: {e}")))?;
                    let belongs_to_file = if omd::relations::node::is_range_key(&reset_node) {
                        omd::relations::node::parent_of(store.state(), &reset_node)
                            == Some(file_node.as_str())
                    } else {
                        reset_node == file_node
                    };
                    if !belongs_to_file {
                        return Err(AppError::usage(format!(
                            "reset target belongs to {reset_node}, not file at {path}"
                        )));
                    }
                    let mut lk = |id: &str| -> Option<pipeline::ResetTargetRecord> {
                        let s = std::fs::read_to_string(root.join(format!("commits/{id}.toml")))
                            .ok()?;
                        let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
                        Some(pipeline::ResetTargetRecord {
                            kind: c.kind,
                            previous_id: c.previous_id,
                            parent_block: c
                                .payload
                                .get("in_block")
                                .and_then(|value| value.as_str())
                                .map(str::to_string),
                        })
                    };
                    match pipeline::reset(&reset_node, &target, &mut lk) {
                        Ok(out) => {
                            // Apply the reset to state FIRST: move tip to the
                            // landing point, dangle removed commits, withdraw
                            // link/adapt records created in the removed segment.
                            // Then the reset marker commit chains onto `actual`.
                            let st = store.state().clone();
                            let mut lk2 = |id: &str| -> Option<(CommitKind, String)> {
                                let s = std::fs::read_to_string(
                                    root.join(format!("commits/{id}.toml")),
                                )
                                .ok()?;
                                let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
                                Some((c.kind, c.previous_id))
                            };
                            // The reset records its own marker ON the landing
                            // point — set the tip to `actual` before the marker
                            // so the marker chains onto it and becomes the tip.
                            let mut pre = st.clone();
                            if omd::relations::node::is_file_key(&reset_node)
                                && !out.actual.is_empty()
                            {
                                let target_commit =
                                    store.read_commit(&out.actual).map_err(|e| {
                                        AppError::usage(format!(
                                            "file reset target {} cannot be read: {e}",
                                            out.actual
                                        ))
                                    })?;
                                let restored = pipeline::file_reset_children_checked(
                                    &target_commit.range_tips,
                                    |tip| {
                                        let mut lookup = |id: &str| {
                                            store.read_commit(id).ok().map(|commit| {
                                                pipeline::ResetTargetRecord {
                                                    kind: commit.kind,
                                                    previous_id: commit.previous_id,
                                                    parent_block: commit
                                                        .payload
                                                        .get("in_block")
                                                        .and_then(|value| value.as_str())
                                                        .map(str::to_string),
                                                }
                                            })
                                        };
                                        pipeline::reset_block_membership(tip, &mut lookup)
                                    },
                                )
                                .map_err(|e| AppError::usage(e.to_string()))?;
                                for (child, tip) in &restored {
                                    let (actual_child, _) =
                                        omd::relations::identity::commit_to_node(&store, tip)
                                            .map_err(|e| {
                                                AppError::usage(format!(
                                                    "file reset child {child} target {tip}: {e}"
                                                ))
                                            })?;
                                    if actual_child != *child {
                                        return Err(AppError::usage(format!(
                                            "file reset child target {tip} belongs to {actual_child}, not {child}"
                                        )));
                                    }
                                    let child_object =
                                        omd::relations::identity::authoritative_object(&store, tip)
                                            .map_err(|e| {
                                                AppError::usage(format!(
                                                    "file reset child {child} target {tip}: {e}"
                                                ))
                                            })?;
                                    if child_object.parent.as_deref() != Some(reset_node.as_str()) {
                                        return Err(AppError::usage(format!(
                                            "file reset child {child} belongs to {:?}, not {reset_node}",
                                            child_object.parent
                                        )));
                                    }
                                }
                                let current_children =
                                    pre.mounts.get(&reset_node).cloned().unwrap_or_default();
                                for child in current_children {
                                    if !target_commit.range_tips.contains_key(&child) {
                                        pipeline::apply_reset_to_state(
                                            &mut pre, &child, "", "", &mut lk2,
                                        );
                                        pre.dirty.remove(&child);
                                        pre.open_blocks.remove(&child);
                                        pre.tags.remove(&child);
                                    }
                                }
                                if let Some(children) = pre.mounts.get_mut(&reset_node) {
                                    children.retain(|child| {
                                        target_commit.range_tips.contains_key(child)
                                    });
                                }
                                for (child, tip) in restored {
                                    let old_tip = pre.tips.get(&child).cloned().unwrap_or_default();
                                    pipeline::apply_reset_to_state(
                                        &mut pre, &child, &tip, &tip, &mut lk2,
                                    );
                                    let children =
                                        pre.mounts.entry(reset_node.clone()).or_default();
                                    if !children.contains(&child) {
                                        children.push(child.clone());
                                    }
                                    if old_tip != tip {
                                        pre.reset_from.insert(tip.clone(), old_tip);
                                    }
                                }
                            }
                            pipeline::apply_reset_to_state(
                                &mut pre,
                                &reset_node,
                                &out.requested,
                                &out.actual,
                                &mut lk2,
                            );
                            if omd::relations::node::is_file_key(&reset_node) {
                                let authoritative = omd::relations::identity::authoritative_object(
                                    &store,
                                    &out.actual,
                                )
                                .map_err(|e| AppError::usage(format!("file reset: {e}")))?;
                                match authoritative.location {
                                    Some(restored) => {
                                        if let Some(occupied) =
                                            store.object_at_path_in(&pre, &restored, false)
                                            && occupied != reset_node
                                        {
                                            return Err(AppError::usage(format!(
                                                "file reset location is already tracked: {restored}"
                                            )));
                                        }
                                        pre.locations.insert(reset_node.clone(), restored);
                                    }
                                    None => {
                                        pre.locations.remove(&reset_node);
                                    }
                                }
                            }
                            if out.actual.is_empty() {
                                for children in pre.mounts.values_mut() {
                                    children.retain(|child| child != &reset_node);
                                }
                                pre.mounts.retain(|_, children| !children.is_empty());
                                pre.dirty.remove(&reset_node);
                                pre.open_blocks.remove(&reset_node);
                                pre.tags.remove(&reset_node);
                                pre.publication += 1;
                                store.set_state_expected(&expected, pre)?;
                                reset_outcome = Some(serde_json::json!({
                                    "requested": out.requested.clone(),
                                    "actual": serde_json::Value::Null,
                                    "requested_id": out.requested,
                                    "actual_id": serde_json::Value::Null,
                                    "warning": out.warning,
                                }));
                                String::new()
                            } else {
                                store.set_state_expected(&expected, pre)?;
                                let mut pl = serde_json::Map::new();
                                pl.insert("requested".into(), out.requested.clone().into());
                                pl.insert("actual".into(), out.actual.clone().into());
                                pl.insert("warning".into(), out.warning.clone().into());
                                let cid = pipeline::commit_marker(
                                    &mut store,
                                    &mut NoProbe,
                                    &OsRng,
                                    clock,
                                    &reset_node,
                                    CommitKind::Reset,
                                    pl,
                                    &expected,
                                )?;
                                reset_outcome = Some(serde_json::json!({
                                    "requested": out.requested.clone(),
                                    "actual": out.actual.clone(),
                                    "requested_id": out.requested,
                                    "actual_id": out.actual,
                                    "warning": out.warning,
                                }));
                                cid
                            }
                        }
                        Err(e) => return Err(AppError::usage(e.to_string())),
                    }
                } else if state_only {
                    pipeline::commit_marker(
                        &mut store,
                        &mut NoProbe,
                        &OsRng,
                        clock,
                        &node,
                        kind,
                        payload,
                        &expected,
                    )?
                } else {
                    // commit verify <file>: a file hash commit MUST NOT pass while
                    // any child range still carries an unhandled obligation or a
                    // dirty mark — the new hash cannot hide a range's debt (4.5).
                    if kind == CommitKind::FileVerify {
                        for (k, ds) in &store.state().dirty {
                            if omd::relations::node::parent_of(store.state(), k)
                                == Some(node.as_str())
                                && (!ds.obligations.is_empty() || !ds.dirty.is_empty())
                            {
                                return Err(AppError::new(
                                    ErrorKind::Check,
                                    format!(
                                        "verify blocked: range {k} still has unhandled obligations/dirty"
                                    ),
                                ));
                            }
                        }
                        // Outstanding (not-yet-persisted) range work also blocks:
                        // re-run the Myers dirty check on each child range tip —
                        // an uncommitted edit that would mark the range dirty
                        // prevents a clean file-verify from hiding it (4.5).
                        for k in store.state().tips.keys() {
                            if omd::relations::node::is_range_key(k)
                                && omd::relations::node::parent_of(store.state(), k)
                                    == Some(node.as_str())
                                && pipeline::range_needs_review(&store, &project_root, k)
                            {
                                return Err(AppError::new(
                                    ErrorKind::Check,
                                    format!(
                                        "verify blocked: range {k} has uncommitted changes needing review"
                                    ),
                                ));
                            }
                        }
                    }
                    let is_new_source =
                        kind == CommitKind::Init && !store.state().tips.contains_key(&node);
                    let (descriptor, recovery, collected) = if is_new_source {
                        let byte_mode = requested_range
                            .is_some_and(|range| range.mode == omd::relations::range::Mode::Byte);
                        let (observation, recovery, collected) =
                            collect_new_source(cli, &context, path, byte_mode)?;
                        (observation, recovery, Some(collected))
                    } else {
                        let evidence_node = if pending_range { &file_node } else { &node };
                        let descriptor = existing_source(cli, &store, evidence_node, path)?;
                        (descriptor.clone(), descriptor, None)
                    };
                    pipeline::commit_source(
                        &mut store,
                        &mut NoProbe,
                        &OsRng,
                        clock,
                        &node,
                        descriptor,
                        recovery,
                        collected,
                        cli.encoding.as_deref(),
                        kind,
                        payload,
                        &expected,
                    )?
                };
                Ok(cid)
            })();
            let cid = match cid_result {
                Ok(cid) => {
                    if combo {
                        succeeded_members.push(successful_member("commit", cid.clone()));
                    }
                    cid
                }
                Err(error) if combo => {
                    return Err(AppError::partial(
                        error,
                        succeeded_members,
                        "business commit",
                        store
                            .state()
                            .open_blocks
                            .get(&node)
                            .cloned()
                            .unwrap_or_default(),
                        &operation_id,
                    ));
                }
                Err(error) => return Err(error),
            };
            // Non-combo new range: the commit itself is the chain root.
            // It published under `range:pending`; rekey to `range:<cid>`.
            if pending_range && !combo {
                node = format!("range:{cid}");
            }
            // Publish local and cross-store link legs while the ATOMIC block
            // is still open. Each failure carries the real error category and
            // current boundary state; no rollback is implied.
            let mut published_link_ids: Vec<String> = Vec::new();
            if combo {
                for endpoint in resolved_endpoints {
                    let result = if let Some(peer_store_id) = endpoint.peer_store_id.as_ref() {
                        if !store.state().peers.contains_key(peer_store_id) {
                            return Err(AppError::partial(
                                AppError::version(format!(
                                    "peer registration changed after preflight: {peer_store_id}"
                                )),
                                succeeded_members,
                                format!("endpoint {}", endpoint.label),
                                store
                                    .state()
                                    .open_blocks
                                    .get(&node)
                                    .cloned()
                                    .unwrap_or_default(),
                                &operation_id,
                            ));
                        }
                        let mut link_id = [0u8; 16];
                        omd::testing::Rng::fill(&OsRng, &mut link_id);
                        let peer = locked_peers.get_mut(peer_store_id).ok_or_else(|| {
                            AppError::version(format!(
                                "locked peer participant is missing: {peer_store_id}"
                            ))
                        })?;
                        let peer_observed = expected.peers.get(peer_store_id).ok_or_else(|| {
                            AppError::version(format!(
                                "caller has no observation for peer {peer_store_id}"
                            ))
                        })?;
                        // Preflight validated the caller's peer observation before any
                        // publication. Later endpoints may target the same locked peer
                        // after an earlier protection advanced its publication; validate
                        // against that known in-operation state instead of misreporting
                        // our own prior write as external drift.
                        let peer_current = omd::records::store::PeerExpected {
                            instance: peer_observed.instance.clone(),
                            mapping_revision: peer_observed.mapping_revision,
                            project_id: peer_observed.project_id.clone(),
                            store_id: peer_observed.store_id.clone(),
                            publication: peer.state().publication,
                            tips: peer.state().tips.clone(),
                            registrations: peer.state().registrations.clone(),
                            peers: peer.state().peers.clone(),
                        };
                        let direction = match endpoint.direction {
                            LinkDirection::From => pipeline::ExternalLinkDirection::PeerToLocal,
                            LinkDirection::To => pipeline::ExternalLinkDirection::LocalToPeer,
                        };
                        match pipeline::commit_external_link_protected(
                            &mut store,
                            peer,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &node,
                            &cid,
                            peer_store_id,
                            &endpoint.root,
                            &endpoint.version,
                            &hex::encode(link_id),
                            reason.as_deref().unwrap_or("combo"),
                            &expected,
                            &peer_current,
                            direction,
                        ) {
                            Ok(published) => {
                                succeeded_members.push(successful_member(
                                    "inbound_protection",
                                    published.credential_id.clone(),
                                ));
                                Ok(published.link_id)
                            }
                            Err(failure) => {
                                if let Some(credential) = failure.credential_id {
                                    succeeded_members
                                        .push(successful_member("inbound_protection", credential));
                                }
                                Err(failure.error.into())
                            }
                        }
                    } else {
                        let (source, source_version, target, target_version) =
                            match endpoint.direction {
                                LinkDirection::From => (
                                    endpoint.root.as_str(),
                                    endpoint.version.as_str(),
                                    node.as_str(),
                                    cid.as_str(),
                                ),
                                LinkDirection::To => (
                                    node.as_str(),
                                    cid.as_str(),
                                    endpoint.root.as_str(),
                                    endpoint.version.as_str(),
                                ),
                            };
                        pipeline::commit_link(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            source,
                            source_version,
                            target,
                            target_version,
                            reason.as_deref().unwrap_or("combo"),
                            &expected,
                        )
                        .map_err(AppError::from)
                    };
                    match result {
                        Ok(link_id) => {
                            succeeded_members.push(successful_member("link", link_id.clone()));
                            published_link_ids.push(link_id);
                        }
                        Err(error) => {
                            return Err(AppError::partial(
                                error,
                                succeeded_members,
                                format!("endpoint {}", endpoint.label),
                                store
                                    .state()
                                    .open_blocks
                                    .get(&node)
                                    .cloned()
                                    .unwrap_or_default(),
                                &operation_id,
                            ));
                        }
                    }
                }

                let mut end_payload = serde_json::Map::new();
                end_payload.insert("path".into(), path.clone().into());
                match pipeline::commit_marker(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    clock,
                    &node,
                    CommitKind::AtomicEnd,
                    end_payload,
                    &expected,
                ) {
                    Ok(end_id) => succeeded_members.push(successful_member("commit", end_id)),
                    Err(error) => {
                        return Err(AppError::partial(
                            error.into(),
                            succeeded_members,
                            "atomic end",
                            store
                                .state()
                                .open_blocks
                                .get(&node)
                                .cloned()
                                .unwrap_or_default(),
                            &operation_id,
                        ));
                    }
                }
            }
            let mut out =
                serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") });
            if let Some(tip) = store.state().tips.get(&node) {
                out.as_object_mut().unwrap().insert(
                    "object".into(),
                    object_projection(&store, &node, tip, true)?,
                );
                out.as_object_mut()
                    .unwrap()
                    .insert("published_commit_id".into(), tip.clone().into());
            }
            if kind == CommitKind::Link
                && let Some(link) = store.state().links.get(&cid)
            {
                out.as_object_mut()
                    .unwrap()
                    .insert("link_id".into(), cid.clone().into());
                out.as_object_mut()
                    .unwrap()
                    .insert("link".into(), link_projection(&store, link));
            }
            if !published_link_ids.is_empty() {
                let link_records: Vec<_> = published_link_ids
                    .iter()
                    .filter_map(|link_id| store.state().links.get(link_id))
                    .map(|link| link_projection(&store, link))
                    .collect();
                out.as_object_mut()
                    .unwrap()
                    .insert("links".into(), published_link_ids.into());
                out.as_object_mut()
                    .unwrap()
                    .insert("link_records".into(), link_records.into());
            }
            if let Some(ro) = reset_outcome {
                out.as_object_mut().unwrap().insert("reset".into(), ro);
            }
            Ok(out)
        }
        Cmd::Verify { .. } => {
            let mut store = open_context_store(&context)?;
            let run_cmd = omd::sources::permission::may_run(&omd::sources::permission::RunChoice {
                cli: cli.run_command,
                config: None,
            });
            store.lock()?;
            let rep = pipeline::verify(&store, &project_root, run_cmd, cli.encoding.as_deref());
            let expected = persist_successful_observations(
                &mut store,
                &context,
                &rep.successful_observations,
            )?;
            let mut value = serde_json::to_value(&rep).unwrap();
            value.as_object_mut().unwrap().insert(
                "diagnostic_issues".into(),
                verification_issues(&store, &rep).into(),
            );
            value
                .as_object_mut()
                .unwrap()
                .insert("expected".into(), serde_json::to_value(expected).unwrap());
            value
                .as_object_mut()
                .unwrap()
                .insert("objects".into(), current_objects(&store)?.into());
            value
                .as_object_mut()
                .unwrap()
                .insert("links".into(), current_links(&store).into());
            Ok(value)
        }
        Cmd::Check { path } => {
            let mut store = open_context_store(&context)?;
            let run_cmd = omd::sources::permission::may_run(&omd::sources::permission::RunChoice {
                cli: cli.run_command,
                config: None,
            });
            store.lock()?;
            let capture = pipeline::verify(&store, &project_root, run_cmd, cli.encoding.as_deref());
            let verification_ok = capture.ok;
            let verification_unverified = capture.unverified.clone();
            let diagnostic_issues = verification_issues(&store, &capture);
            let identity = capture.identity.clone();
            let expected = persist_successful_observations(
                &mut store,
                &context,
                &capture.successful_observations,
            )?;
            if !identity.is_empty() {
                return Ok(serde_json::json!({
                    "ok": false,
                    "check": {
                        "identity": identity,
                        "unverified": verification_unverified,
                        "diagnostic_issues": diagnostic_issues,
                        "expected": expected,
                    }
                }));
            }
            // Resolve every recorded Import scope against the live FS, then
            // report which in-scope files carry unexpired confirmed coverage.
            // New members auto-enter statistics because scope re-resolves now.
            let mut scopes: Vec<(String, Vec<String>)> = Vec::new();
            for node in store.state().tips.keys() {
                if !omd::relations::node::is_file_key(node) {
                    continue;
                }
                let root_id = node.split_once(':').map(|(_, id)| id).unwrap_or("");
                let Ok(root_commit) = store.read_commit(root_id) else {
                    continue;
                };
                if root_commit.kind != omd::records::commit::CommitKind::Import {
                    continue;
                }
                let Some(scope) = import_scope(&store, node)? else {
                    continue;
                };
                let path = omd::relations::node::path_of(store.state(), node)
                    .unwrap_or("")
                    .to_string();
                scopes.push((path, scope));
            }
            let mut report = serde_json::Map::new();
            let mut all_files = Vec::new();
            let mut scope_problems = Vec::new();
            let proj_root = project_root.clone();
            // A dir-import enters members into the statistics SCOPE (the
            // denominator) — it does NOT confer confirmed content coverage.
            // `tracked` = the file has its own confirmed tracked node; scope
            // membership only brings it into the check's denominator.
            for (dir, pats) in &scopes {
                let base = proj_root.join(dir);
                let single_file = base.is_file();
                let scope = omd::sources::scope::resolve(&base, pats);
                for f in &scope.files {
                    let path = if single_file {
                        dir.clone()
                    } else if dir.is_empty() {
                        f.display().to_string()
                    } else {
                        format!("{}/{}", dir.trim_end_matches('/'), f.display())
                    };
                    let tracked_node = store.file_at_path(&path).map(str::to_string);
                    let object = tracked_node
                        .as_deref()
                        .and_then(|node| store.state().tips.get(node).map(|tip| (node, tip)))
                        .map(|(node, tip)| object_projection(&store, node, tip, true))
                        .transpose()?;
                    all_files.push(serde_json::json!({
                        "file": f.display().to_string(),
                        "scope": dir,
                        "tracked": tracked_node.is_some(),
                        "in_scope": true,
                        "object": object,
                    }));
                }
                scope_problems.extend(scope.problems);
            }
            let scope_incomplete = !scope_problems.is_empty();
            if scope_incomplete {
                report.insert("problems".into(), scope_problems.into());
                report.insert("incomplete".into(), true.into());
            }
            // A tracked file node whose source vanished reports `incomplete`
            // — never a silent empty `files:[]` success. This is check's own
            // honesty floor, independent of `verify`'s `missing` diagnostic.
            let mut any_missing = false;
            for (node, path) in &store.state().locations {
                if store.file_at_path(path) != Some(node.as_str()) {
                    continue;
                }
                let Some(tip) = store.state().tips.get(node) else {
                    continue;
                };
                let is_tombstone = store
                    .read_commit(tip)
                    .map(|c| c.kind == omd::records::commit::CommitKind::Delete)
                    .unwrap_or(false);
                if !is_tombstone && !proj_root.join(path).exists() {
                    all_files.push(serde_json::json!({
                        "file": path, "tracked": true, "in_scope": true,
                        "status": "incomplete", "reason": "source missing",
                        "object": object_projection(&store, node, tip, true)?,
                    }));
                    any_missing = true;
                }
            }
            report.insert("files".into(), all_files.into());
            if any_missing {
                report.insert("incomplete".into(), true.into());
            }

            // Tag-link rules: each declared `spec->code` (one-way) or
            // `spec<->code` (two-way) computes *position coverage* — the
            // union of confirmed linked-range positions over the source
            // tag's member content — never object counts. A rule reports a
            // named check item; skip never confirms; extra reverse links
            // under a one-way rule are not violations.
            let mut rules_out = Vec::new();
            let mut check_failed = false;
            for (name, rule) in &store.state().tag_rules {
                let (src_tag, tgt_tag, two_way) = if let Some((a, b)) = name.split_once("<->") {
                    (a.trim(), b.trim(), true)
                } else if let Some((a, b)) = name.split_once("->") {
                    (a.trim(), b.trim(), false)
                } else {
                    continue;
                };
                let forward = coverage_direction(
                    &store,
                    &proj_root,
                    src_tag,
                    tgt_tag,
                    &capture.successful_observations,
                    &capture.dirty,
                    cli.encoding.as_deref(),
                )?;
                let reverse = two_way
                    .then(|| {
                        coverage_direction(
                            &store,
                            &proj_root,
                            tgt_tag,
                            src_tag,
                            &capture.successful_observations,
                            &capture.dirty,
                            cli.encoding.as_deref(),
                        )
                    })
                    .transpose()?;
                let forward_status = forward["status"].as_str().unwrap_or("incomplete");
                let reverse_status = reverse
                    .as_ref()
                    .and_then(|value| value["status"].as_str())
                    .unwrap_or("pass");
                let incomplete = forward_status == "incomplete" || reverse_status == "incomplete";
                let pass = forward_status == "pass" && reverse_status == "pass";
                let status = if rule.skip {
                    "skipped"
                } else if incomplete {
                    "incomplete"
                } else if pass {
                    "pass"
                } else {
                    "fail"
                };
                if !rule.skip && status != "pass" && rule.level == "fail" {
                    check_failed = true;
                }
                rules_out.push(serde_json::json!({
                    "rule": name,
                    "level": rule.level,
                    "status": status,
                    "coverage": {
                        "forward": forward,
                        "reverse": reverse,
                    },
                }));
            }
            report.insert("rules".into(), rules_out.into());
            report.insert("objects".into(), current_objects(&store)?.into());
            report.insert("links".into(), current_links(&store).into());
            if let Some(p) = path {
                report.insert("query".into(), p.clone().into());
            }
            report.insert("unverified".into(), verification_unverified.into());
            report.insert("diagnostic_issues".into(), diagnostic_issues.into());
            report.insert("expected".into(), serde_json::to_value(expected).unwrap());
            Ok(
                serde_json::json!({ "ok": verification_ok && !check_failed && !scope_incomplete, "check": report }),
            )
        }
        Cmd::Rename { source, target } => {
            let (source, _) = omd::sources::projects::project_path(&project_root, source)?;
            let (target, _) = omd::sources::projects::project_path(&project_root, target)?;
            let mut store = open_context_store(&context)?;
            let cid = pipeline::commit_lifecycle(
                &mut store,
                &mut NoProbe,
                &OsRng,
                &SystemClock,
                CommitKind::Rename,
                &source,
                Some(&target),
                "",
                &expected,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "commit": cid,
                "kind": "Rename",
                "object": commit_projection(&store, &cid, true)?,
            }))
        }
        Cmd::Copy { source, target } => {
            // copy = a fresh init on the target path — new identity, source
            // untouched. Target file must exist to observe.
            let (target, current_target) =
                omd::sources::projects::project_path(&project_root, target)?;
            let mut store = open_context_store(&context)?;
            if store.file_at_path(&target).is_some() {
                return Err(AppError::usage(format!(
                    "copy target is already tracked: {target}"
                )));
            }
            let node = "file:pending";
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), target.clone().into());
            let cid = pipeline::commit_file(
                &mut store,
                &mut NoProbe,
                &OsRng,
                &SystemClock,
                node,
                &current_target,
                CommitKind::Init,
                payload,
                &expected,
                None,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "commit": cid,
                "kind": "Init",
                "copied_from": source,
                "object": commit_projection(&store, &cid, true)?,
            }))
        }
        Cmd::Note {
            action,
            commit_id,
            target,
            text,
        } => {
            let mut store = open_context_store(&context)?;
            // Seq is max-of-notes+1 — two concurrent writers must serialize
            // or they'd compute the same seq. Lock around the read+append.
            store.lock()?;
            if action != "list" {
                store.require_identity()?;
                store.check_expected(&expected)?;
            }
            match action.as_str() {
                "add" | "patch" | "delete" => {
                    // Publication order is monotonic, never the clock — the
                    // next seq is the max already-published seq + 1 (the
                    // store publication counter counts commits, not notes).
                    let seq = omd::records::notes::list_for(&root, commit_id)
                        .iter()
                        .map(|n| n.seq)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    let mut idb = [0u8; 16];
                    omd::testing::Rng::fill(&OsRng, &mut idb);
                    let note = omd::records::notes::Note {
                        id: hex::encode(idb),
                        timestamp: omd::records::commit::Commit::format_timestamp(
                            omd::testing::Clock::now(&SystemClock).0,
                        ),
                        commit_id: commit_id.clone(),
                        kind: action.clone(),
                        target_note_id: target.clone().unwrap_or_default(),
                        text: text.clone().unwrap_or_default(),
                        seq,
                    };
                    let nid = omd::records::notes::append(&root, &note)
                        .map_err(|e| AppError::io(e.to_string()))?;
                    Ok(serde_json::json!({ "ok": true, "note": nid, "kind": action }))
                }
                "list" => {
                    let notes = omd::records::notes::list_for(&root, commit_id);
                    Ok(serde_json::json!({ "ok": true, "commit_id": commit_id, "notes": notes }))
                }
                other => Err(AppError::usage(format!("unknown note action: {other}"))),
            }
        }
        Cmd::Replace { commit_id } => {
            let mut store = open_context_store(&context)?;
            store.lock()?;
            store.require_identity()?;
            store.check_expected(&expected)?;

            let selected = omd::relations::identity::resolve_commit_id(&store, commit_id)
                .map_err(|error| AppError::usage(format!("replace commit: {error}")))?;
            let commit = store.read_commit(&selected)?;
            if commit.content_ref == "empty" {
                return Err(AppError::usage(
                    "replace requires a commit with a complete source version",
                ));
            }
            let version = store.read_version(&commit.content_ref)?;
            let recorded = store.recover_version_bytes(&commit.content_ref)?;
            let descriptor = source_fields(cli)
                .descriptor(None, true)
                .map_err(|error| AppError::usage(error.to_string()))?
                .expect("replace source type is required");
            let cwd = std::env::current_dir()
                .map_err(|error| AppError::io(format!("current directory: {error}")))?;
            let descriptor = omd::sources::normalize_descriptor(descriptor, &cwd, &project_root)?;
            let candidate =
                omd::sources::collect(&descriptor, &cwd, &project_root, false, None)?.bytes;
            if candidate != recorded
                || candidate.len() as u64 != version.len
                || omd::records::version::SourceVersion::content_sha256(&candidate)
                    != version.sha256
            {
                return Err(AppError::new(
                    ErrorKind::Check,
                    "replace refused: new source differs from recorded complete content",
                ));
            }

            let retained: std::collections::BTreeSet<_> =
                store.state().retained.iter().cloned().collect();
            let mut affected = Vec::new();
            for id in retained {
                if store
                    .read_commit(&id)
                    .is_ok_and(|record| record.content_ref == commit.content_ref)
                {
                    affected.push(id);
                }
            }
            affected.sort();
            let mut id = [0u8; 16];
            omd::testing::Rng::fill(&OsRng, &mut id);
            let binding = omd::records::binding::Binding {
                format: "omd.binding/2".into(),
                id: hex::encode(id),
                version_id: commit.content_ref.clone(),
                recovery: descriptor,
                seq: store.state().publication + 1,
                affected: affected.clone(),
            };
            store.publish_binding(&expected, &binding)?;
            Ok(serde_json::json!({
                "ok": true,
                "binding": binding.id,
                "version": binding.version_id,
                "affected": affected,
            }))
        }
        Cmd::Register {
            store_id,
            project_root: peer_project_root,
            metadata_dir,
            peer_expected,
        } => {
            let mut store = open_context_store(&context)?;
            let peer_project_root = std::fs::canonicalize(peer_project_root)
                .map_err(|error| AppError::io(format!("peer project location: {error}")))?;
            let peer_meta = std::fs::canonicalize(metadata_dir)
                .map_err(|error| AppError::io(format!("peer metadata location: {error}")))?;
            let mut peer_store = Store::open_existing(&peer_meta)?;
            let peer_observed = parse_expected_arg(peer_expected)?;
            let peer_identity = peer_store.identity();
            if peer_identity.store_id != *store_id
                || peer_observed.project_id != peer_identity.project_id
                || peer_observed.store_id != peer_identity.store_id
            {
                return Err(AppError::usage(format!(
                    "peer store identity mismatch: requested {store_id}, found {}",
                    peer_identity.store_id
                )));
            }
            let peer_instance = omd::sources::projects::context_instance(
                &peer_project_root,
                &peer_meta,
                &peer_identity,
            )?;
            if peer_observed.instance != peer_instance {
                return Err(AppError::version(
                    "peer physical instance changed since verify/check",
                ));
            }
            let local_root = std::fs::canonicalize(store.root())
                .map_err(|error| AppError::io(format!("local metadata location: {error}")))?;
            let peer_root = std::fs::canonicalize(peer_store.root())
                .map_err(|error| AppError::io(format!("peer metadata location: {error}")))?;
            if local_root == peer_root {
                return Err(AppError::usage("peer store must be a distinct authority"));
            }
            if local_root < peer_root {
                store.lock()?;
                peer_store.lock()?;
            } else {
                peer_store.lock()?;
                store.lock()?;
            }
            store.check_metadata_expected(&expected)?;
            if peer_store.state().publication != peer_observed.publication
                || peer_store.state().tips != peer_observed.tips
                || peer_store.state().registrations != peer_observed.registrations
                || peer_store.state().peers != peer_observed.peer_registrations
            {
                return Err(AppError::version(
                    "peer publication/tips/registration changed since verify/check",
                ));
            }
            let registration_id = omd::records::cross::register_peer(
                &mut store,
                &peer_identity.project_id,
                &peer_identity.store_id,
            )?;
            let cwd = store
                .config_cwd()
                .ok_or_else(|| AppError::usage("peer registration requires local config"))?;
            let previous = omd::sources::projects::load_peers(cwd)?
                .into_iter()
                .find(|map| {
                    map.owner_store_id == store.state().store_id
                        && map.peer_store_id == peer_identity.store_id
                });
            let mapping = omd::sources::projects::PeerMap {
                owner_store_id: store.state().store_id.clone(),
                peer_project_id: peer_identity.project_id,
                peer_store_id: peer_identity.store_id.clone(),
                project_root: peer_project_root,
                metadata_root: peer_meta,
                peer_registration_id: registration_id.clone(),
                revision: previous.map_or(1, |map| map.revision + 1),
            };
            omd::sources::projects::save_peer_mapping(cwd, mapping)?;
            Ok(serde_json::json!({
                "ok": true,
                "registered": store_id,
                "registration_id": registration_id,
            }))
        }
        Cmd::Project { .. } => unreachable!("project commands return before context selection"),
        Cmd::Protect {
            target,
            peer,
            record,
            link,
        } => {
            if record.is_empty() || record.contains(char::is_whitespace) {
                return Err(AppError::usage(
                    "protect requires a non-empty single-token record id",
                ));
            }
            if link.is_empty() || link.contains(char::is_whitespace) {
                return Err(AppError::usage(
                    "protect requires a non-empty single-token link id",
                ));
            }
            let mut store = open_context_store(&context)?;
            let mut peers = std::collections::BTreeMap::new();
            peers.insert(peer.clone(), peer_store(&store, peer, &expected)?);
            lock_participants(&mut store, &mut peers)?;
            store.check_metadata_expected(&expected)?;
            let consumer = peers
                .get_mut(peer)
                .ok_or_else(|| AppError::usage("consumer peer mapping disappeared"))?;
            let observed = expected.peers.get(peer).ok_or_else(|| {
                AppError::version(format!("caller has no peer observation for {peer}"))
            })?;
            validate_peer_locked(&store, consumer, observed)?;
            let selected = omd::relations::identity::resolve_commit_id(&store, target)
                .map_err(|error| AppError::usage(format!("protection target: {error}")))?;
            let (target_root, _) = omd::relations::identity::commit_to_node(&store, &selected)
                .map_err(|error| AppError::usage(format!("protection target: {error}")))?;
            if !omd::relations::node::is_range_key(&target_root) {
                return Err(AppError::usage(
                    "protection target must identify a range version",
                ));
            }
            let consumer_registration_id = store
                .state()
                .peers
                .get(peer)
                .cloned()
                .ok_or_else(|| AppError::version(format!("peer not registered: {peer}")))?;
            let credential = omd::records::cross::persist_inbound(
                &mut store,
                &consumer.identity().project_id,
                &consumer.state().store_id,
                &consumer_registration_id,
                record,
                link,
                &target_root,
                &selected,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "credential": credential,
                "record_id": record,
                "link_id": link,
                "target_root": target_root,
                "target_version": selected,
            }))
        }
        Cmd::Activate => {
            let mut store = open_context_store(&context)?;
            if omd::records::cross::activated(&store) {
                return Err(AppError::usage(
                    "selected instance is already writable; correct its location instead of activating a copy",
                ));
            }
            let mut required = std::collections::BTreeMap::<
                String,
                Vec<(omd::records::store::Link, String, String)>,
            >::new();
            for link in store.state().links.values() {
                for (endpoint, version) in [
                    (&link.source, &link.source_version),
                    (&link.target, &link.target_version),
                ] {
                    let Some(rest) = endpoint.strip_prefix("peer:") else {
                        continue;
                    };
                    let Some((peer_store_id, target_root)) = rest.split_once(':') else {
                        return Err(AppError::new(
                            ErrorKind::Check,
                            format!("malformed copied peer endpoint: {endpoint}"),
                        ));
                    };
                    if !omd::relations::node::is_range_key(target_root) {
                        return Err(AppError::new(
                            ErrorKind::Check,
                            format!("copied peer endpoint is not a range: {endpoint}"),
                        ));
                    }
                    required
                        .entry(peer_store_id.to_string())
                        .or_default()
                        .push((link.clone(), target_root.to_string(), version.clone()));
                }
            }
            let mut peers = std::collections::BTreeMap::new();
            for peer_store_id in required.keys() {
                peers.insert(
                    peer_store_id.clone(),
                    peer_store(&store, peer_store_id, &expected)?,
                );
            }
            lock_participants(&mut store, &mut peers)?;
            store.check_metadata_expected(&expected)?;
            for (peer_store_id, peer) in &mut peers {
                let observed = expected.peers.get(peer_store_id).ok_or_else(|| {
                    AppError::version(format!("required peer {peer_store_id} was not observed"))
                })?;
                validate_peer_locked(&store, peer, observed)?;
            }
            let project_id = store.identity().project_id;
            let old_store_id = store.state().store_id.clone();
            let mut new_store_id = omd::records::cross::new_store_id(&OsRng);
            while new_store_id == project_id || new_store_id == old_store_id {
                new_store_id = omd::records::cross::new_store_id(&OsRng);
            }
            let cwd = store
                .config_cwd()
                .ok_or_else(|| AppError::usage("copy activation requires local configuration"))?
                .to_path_buf();
            let local_project_root = store
                .project_root()
                .ok_or_else(|| AppError::usage("copy activation requires a selected project root"))?
                .to_path_buf();
            let local_metadata_root = store.metadata_root().to_path_buf();
            let prior_peer_maps = omd::sources::projects::load_peers(&cwd)?;
            let mut protections = Vec::new();
            for (peer_store_id, links) in &required {
                let peer = peers.get_mut(peer_store_id).ok_or_else(|| {
                    AppError::version(format!("required peer disappeared: {peer_store_id}"))
                })?;
                let consumer_registration_id =
                    omd::records::cross::register_peer(peer, &project_id, &new_store_id)?;
                let peer_project_root = prior_peer_maps
                    .iter()
                    .find(|mapping| {
                        mapping.owner_store_id == old_store_id
                            && mapping.peer_store_id == *peer_store_id
                    })
                    .map(|mapping| mapping.project_root.clone())
                    .ok_or_else(|| {
                        AppError::version(format!(
                            "required peer mapping is missing: {peer_store_id}"
                        ))
                    })?;
                let peer_mapping = omd::sources::projects::PeerMap {
                    owner_store_id: peer_store_id.clone(),
                    peer_project_id: project_id.clone(),
                    peer_store_id: new_store_id.clone(),
                    project_root: local_project_root.clone(),
                    metadata_root: local_metadata_root.clone(),
                    peer_registration_id: consumer_registration_id.clone(),
                    revision: 1,
                };
                omd::sources::projects::save_peer_mapping(&cwd, peer_mapping)?;
                let outbound_registration_id = store
                    .state()
                    .peers
                    .get(peer_store_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::version(format!(
                            "copied peer registration is missing: {peer_store_id}"
                        ))
                    })?;
                omd::sources::projects::save_peer_mapping(
                    &cwd,
                    omd::sources::projects::PeerMap {
                        owner_store_id: new_store_id.clone(),
                        peer_project_id: peer.identity().project_id,
                        peer_store_id: peer_store_id.clone(),
                        project_root: peer_project_root,
                        metadata_root: peer.metadata_root().to_path_buf(),
                        peer_registration_id: outbound_registration_id,
                        revision: 1,
                    },
                )?;
                for (link, target_root, target_version) in links {
                    let credential = omd::records::cross::persist_inbound(
                        peer,
                        &project_id,
                        &new_store_id,
                        &consumer_registration_id,
                        &link.created_by,
                        &link.link_id,
                        target_root,
                        target_version,
                    )?;
                    protections.push(credential);
                }
            }
            let copied_registrations =
                omd::records::cross::copied_project_registrations(&store, &new_store_id)?;
            let new_identity = omd::records::store::StoreIdentity {
                project_id: project_id.clone(),
                store_id: new_store_id.clone(),
                registrations: copied_registrations.clone(),
                activated: true,
            };
            omd::sources::projects::retarget_store_instance(
                &cwd,
                &local_metadata_root,
                &project_id,
                &old_store_id,
                &new_store_id,
            )?;
            omd::sources::projects::authorize_instance(
                &cwd,
                &local_project_root,
                &local_metadata_root,
                &new_identity,
            )?;
            store.publish_copy_activation(&expected, &new_store_id, copied_registrations)?;
            Ok(serde_json::json!({
                "ok": true,
                "store_id": new_store_id,
                "previous_store_id": old_store_id,
                "protections": protections,
            }))
        }
        Cmd::Gc { content } => {
            let mut store = open_context_store(&context)?;
            if !omd::records::cross::activated(&store) {
                return Err(
                    "store is an unregistered copy: gc refused until `omd activate`".into(),
                );
            }
            let inbound_records: Vec<_> = store
                .state()
                .inbound
                .values()
                .map(|id| omd::records::cross::read_inbound(&store, id))
                .collect::<Result<_, _>>()?;
            let mut consumers = std::collections::BTreeMap::new();
            for credential in &inbound_records {
                if consumers.contains_key(&credential.consumer_store_id)
                    || !expected.peers.contains_key(&credential.consumer_store_id)
                {
                    continue;
                }
                if let Ok(consumer) = peer_store(&store, &credential.consumer_store_id, &expected) {
                    consumers.insert(credential.consumer_store_id.clone(), consumer);
                }
            }
            lock_participants(&mut store, &mut consumers)?;
            store.check_metadata_expected(&expected)?;
            for (consumer_store_id, consumer) in &mut consumers {
                validate_peer_locked(
                    &store,
                    consumer,
                    expected.peers.get(consumer_store_id).ok_or_else(|| {
                        AppError::version(format!(
                            "caller has no observation for consumer {consumer_store_id}"
                        ))
                    })?,
                )?;
            }
            let mut protected_credentials = std::collections::BTreeMap::<String, String>::new();
            let mut orphan_credentials = std::collections::BTreeSet::<String>::new();
            for credential in &inbound_records {
                let Some(consumer) = consumers.get(&credential.consumer_store_id) else {
                    protected_credentials.insert(
                        credential.credential_id.clone(),
                        "consumer unavailable, unmapped, or not observed".into(),
                    );
                    continue;
                };
                let registration_matches = store
                    .state()
                    .peers
                    .get(&credential.consumer_store_id)
                    .is_some_and(|selected| selected == &credential.consumer_registration_id)
                    && omd::records::cross::read_peer(&store, &credential.consumer_registration_id)
                        .is_ok_and(|registration| {
                            registration.peer_store_id == credential.consumer_store_id
                                && registration.peer_project_id == credential.consumer_project_id
                        });
                if !registration_matches {
                    protected_credentials.insert(
                        credential.credential_id.clone(),
                        "consumer registration changed or cannot be proven".into(),
                    );
                    continue;
                }
                let expected_target =
                    format!("peer:{}:{}", store.state().store_id, credential.target_root);
                if !consumer.state().retained.contains(&credential.record_id) {
                    orphan_credentials.insert(credential.credential_id.clone());
                    continue;
                }
                let Ok(record) = consumer.read_commit(&credential.record_id) else {
                    protected_credentials.insert(
                        credential.credential_id.clone(),
                        "consumer record is retained but unreadable".into(),
                    );
                    continue;
                };
                let source_matches = record
                    .payload
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    == Some(expected_target.as_str())
                    && record
                        .payload
                        .get("source_version")
                        .and_then(serde_json::Value::as_str)
                        == Some(credential.target_version.as_str());
                let target_matches = record
                    .payload
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    == Some(expected_target.as_str())
                    && record
                        .payload
                        .get("target_version")
                        .and_then(serde_json::Value::as_str)
                        == Some(credential.target_version.as_str());
                let record_matches = record.kind == CommitKind::Link
                    && record
                        .payload
                        .get("link_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(credential.link_id.as_str())
                    && (source_matches || target_matches);
                if !record_matches {
                    protected_credentials.insert(
                        credential.credential_id.clone(),
                        "consumer record identity or protected target cannot be proven".into(),
                    );
                    continue;
                }
                let effective =
                    consumer
                        .state()
                        .links
                        .get(&credential.link_id)
                        .is_some_and(|link| {
                            let source_matches = link.source == expected_target
                                && link.source_version == credential.target_version;
                            let target_matches = link.target == expected_target
                                && link.target_version == credential.target_version;
                            link.link_id == credential.link_id
                                && link.created_by == credential.record_id
                                && (source_matches || target_matches)
                        });
                if effective {
                    protected_credentials.insert(
                        credential.credential_id.clone(),
                        "effective consumer link".into(),
                    );
                } else {
                    protected_credentials.insert(
                        credential.credential_id.clone(),
                        "exact consumer record remains retained".into(),
                    );
                }
            }
            // Content gc: a local content copy is collectible only if no
            // version still references it AND no binding still points a
            // version at it. Conservative: unreadable peers + inbound
            // credentials retain their targets.
            let mut released = Vec::new();
            if *content {
                let mut protected = std::collections::HashSet::new();
                let mut versions_by_hash: std::collections::BTreeMap<
                    String,
                    Vec<omd::records::version::SourceVersion>,
                > = std::collections::BTreeMap::new();
                if let Ok(entries) = std::fs::read_dir(root.join("versions")) {
                    for entry in entries {
                        let entry = entry.map_err(|error| AppError::io(error.to_string()))?;
                        let id = entry
                            .path()
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .ok_or_else(|| AppError::usage("invalid source version filename"))?
                            .to_string();
                        let version = store.read_version(&id)?;
                        versions_by_hash
                            .entry(version.sha256.clone())
                            .or_default()
                            .push(version);
                    }
                }
                // Inbound credentials protect their targets' content — and
                // the TRANSITIVE closure: a protected target's own
                // previous_id / content / link basis must be retained so the
                // target remains recoverable, never just the direct object.
                for credential_id in protected_credentials.keys() {
                    let Ok(cred) = omd::records::cross::read_inbound(&store, credential_id) else {
                        continue;
                    };
                    protected.insert(cred.target_version.clone());
                    // Walk the protected commit's chain basis.
                    if let Ok(c) = store.read_commit(&cred.target_version) {
                        if let Ok(v) = store.read_version(&c.content_ref) {
                            protected.insert(v.sha256.clone());
                        }
                        let mut prev = c.previous_id.clone();
                        while !prev.is_empty() {
                            if let Ok(pc) = store.read_commit(&prev) {
                                if let Ok(v) = store.read_version(&pc.content_ref) {
                                    protected.insert(v.sha256.clone());
                                }
                                prev = pc.previous_id.clone();
                            } else {
                                break;
                            }
                        }
                    }
                }
                let mut releasable = Vec::new();
                if let Ok(entries) = std::fs::read_dir(root.join("content")) {
                    for entry in entries {
                        let entry = entry.map_err(|error| AppError::io(error.to_string()))?;
                        let hash = entry.file_name().to_string_lossy().to_string();
                        if protected.contains(&hash) {
                            continue;
                        }
                        let recoverable = if let Some(versions) = versions_by_hash.get(&hash) {
                            let mut all = true;
                            for version in versions {
                                all &= store.verify_exact_git_recovery(version)?;
                            }
                            all
                        } else {
                            true
                        };
                        if recoverable {
                            releasable.push((entry.path(), hash));
                        }
                    }
                }
                for (path, hash) in releasable {
                    std::fs::remove_file(path).map_err(|error| AppError::io(error.to_string()))?;
                    released.push(hash);
                }
            }
            // Commit/note gc (always, not just --content): collect dangling
            // commits — in `published` but unreachable from any tip and not
            // referenced by a live record / inbound credential — and drop
            // their note files. A dangling commit referenced by an effective
            // record or a protection target is RETAINED, never collected.
            let mut collected = Vec::new();
            {
                // Reachable set: every tip + ancestors.
                let mut reachable = std::collections::BTreeSet::new();
                for tip in store.state().tips.values() {
                    let mut cur = tip.clone();
                    while !cur.is_empty() && reachable.insert(cur.clone()) {
                        cur = commit_prev(&root, &cur).unwrap_or_default();
                    }
                }
                // Protected targets stay: inbound credentials, and any
                // dangling commit an *effective record* still references —
                // a link's source/target node keys, or a note's commit_id.
                let mut keep = reachable.clone();
                for credential_id in protected_credentials.keys() {
                    if let Ok(cred) = omd::records::cross::read_inbound(&store, credential_id) {
                        keep.insert(cred.target_version);
                    }
                }
                for link in store.state().links.values() {
                    keep.insert(link.source_version.clone());
                    keep.insert(link.target_version.clone());
                    keep.insert(link.created_by.clone());
                }
                // Notes referencing a commit keep it (evidence of record).
                if let Ok(rd) = std::fs::read_dir(root.join("notes")) {
                    for e in rd.flatten() {
                        if let Ok(txt) = std::fs::read_to_string(e.path())
                            && let Ok(n) = toml::from_str::<omd::records::notes::Note>(&txt)
                        {
                            keep.insert(n.commit_id.clone());
                        }
                    }
                }
                // Every protected commit keeps its immutable ancestry and
                // every child-tip snapshot needed by a future file reset.
                let mut queue: Vec<String> = keep.iter().cloned().collect();
                while let Some(current) = queue.pop() {
                    let Ok(commit) = store.read_commit(&current) else {
                        continue;
                    };
                    if !commit.previous_id.is_empty() && keep.insert(commit.previous_id.clone()) {
                        queue.push(commit.previous_id.clone());
                    }
                    for child_tip in commit.range_tips.values() {
                        if keep.insert(child_tip.clone()) {
                            queue.push(child_tip.clone());
                        }
                    }
                }
                // Candidates: commits on disk not reachable/kept.
                if let Ok(rd) = std::fs::read_dir(root.join("commits")) {
                    for e in rd.flatten() {
                        let id = e.path().file_stem().unwrap().to_string_lossy().to_string();
                        if !keep.contains(&id) {
                            collected.push(id);
                        }
                    }
                }
                if !collected.is_empty() || !orphan_credentials.is_empty() {
                    let collected_set: std::collections::BTreeSet<_> =
                        collected.iter().cloned().collect();
                    let mut state = store.state().clone();
                    state.retained.retain(|id| !collected_set.contains(id));
                    state.reset_from.retain(|id, from| {
                        !collected_set.contains(id) && !collected_set.contains(from)
                    });
                    for pending in state.link_pending.values_mut() {
                        pending.retain(|id| !collected_set.contains(id));
                    }
                    state
                        .inbound
                        .retain(|credential, _| !orphan_credentials.contains(credential));
                    state.publication += 1;
                    store.set_state_expected(&expected, state)?;
                    for credential in &orphan_credentials {
                        let _ =
                            std::fs::remove_file(root.join(format!("inbound/{credential}.toml")));
                    }
                    for id in &collected {
                        let _ = std::fs::remove_file(root.join(format!("commits/{id}.toml")));
                    }
                    if let Ok(entries) = std::fs::read_dir(root.join("notes")) {
                        for entry in entries.flatten() {
                            if let Ok(text) = std::fs::read_to_string(entry.path())
                                && let Ok(note) = toml::from_str::<omd::records::notes::Note>(&text)
                                && collected_set.contains(&note.commit_id)
                            {
                                let _ = std::fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }
            // Report WHY each target is retained — the offline consumer's
            // peer_store_id + the protected target, never a bare count. The
            // credential is durable even when the consumer is unreachable.
            let protection_reasons: Vec<serde_json::Value> = protected_credentials
                .iter()
                .filter_map(|(id, reason)| {
                    omd::records::cross::read_inbound(&store, id)
                        .ok()
                        .map(|c| (c, reason))
                })
                .map(|(c, reason)| {
                    serde_json::json!({
                        "consumer": c.consumer_store_id,
                        "target_root": c.target_root,
                        "target_version": c.target_version,
                        "reason": reason,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "ok": true, "released": released, "collected_commits": collected,
                "collected_protections": orphan_credentials,
                "protected_count": protected_credentials.len(),
                "protection_reasons": protection_reasons,
            }))
        }
        Cmd::Reindex => {
            // Rebuild derived index from authority only. Cache is scoped by
            // physical instance plus exact publication/registration state;
            // it is never evidence or a second source of truth.
            let store = open_context_store(&context)?;
            let manifest = std::fs::read_to_string(root.join("published")).unwrap_or_default();
            let mut rows = Vec::new();
            for id in manifest.lines().filter(|l| !l.is_empty()) {
                if let Ok(c) = store.read_commit(id) {
                    rows.push(format!(
                        "{}\t{}\t{}",
                        id,
                        omd::records::commit::kind_name(c.kind),
                        c.content_ref
                    ));
                }
            }
            rows.sort();
            let cwd = std::env::current_dir()
                .map_err(|error| AppError::io(format!("current directory: {error}")))?;
            let registration =
                serde_json::to_vec(&(&store.state().registrations, context.mapping_revision))
                    .map_err(|error| AppError::io(error.to_string()))?;
            let registration = omd::records::version::SourceVersion::content_sha256(&registration);
            let cache_root = omd::sources::discovery::cache_dir(&cwd)
                .join("indexes")
                .join(&context.instance);
            std::fs::create_dir_all(&cache_root).map_err(|e| AppError::io(e.to_string()))?;
            let cache_file = cache_root.join(format!(
                "{}-{}-{}.txt",
                store.state().store_id,
                store.state().publication,
                registration
            ));
            std::fs::write(&cache_file, rows.join("\n"))
                .map_err(|e| AppError::io(e.to_string()))?;
            Ok(serde_json::json!({
                "ok": true,
                "indexed": rows.len(),
                "cache_instance": context.instance,
                "cache_file": cache_file,
            }))
        }
        Cmd::Log { id } => {
            let store = open_context_store(&context)?;
            let chain = log_chain(&store, id)?;
            let records: Result<Vec<_>, _> = chain
                .iter()
                .map(|commit_id| commit_projection(&store, commit_id, false))
                .collect();
            let selected = chain.first().cloned().unwrap_or_default();
            Ok(serde_json::json!({
                "ok": true,
                "node": id,
                "selected": if selected.is_empty() { serde_json::Value::Null } else { commit_projection(&store, &selected, false)? },
                "chain": chain,
                "records": records?,
            }))
        }
        Cmd::Tree { id, level } => {
            let store = open_context_store(&context)?;
            // `--level file` caps at file level; a number caps depth.
            let (max_depth, file_only) = match level.as_deref() {
                Some("file") => (1usize, true),
                Some(n) => (n.parse().unwrap_or(32), false),
                None => (32usize, false),
            };
            mount_tree(&store, id.as_deref(), max_depth, file_only)
        }
        Cmd::List { dangling } => {
            let store = open_context_store(&context)?;
            let objects = current_objects(&store)?;
            let links = current_links(&store);
            if *dangling {
                let dangling = dangling_ids(&store, &root);
                let records: Result<Vec<_>, _> = dangling
                    .iter()
                    .map(|commit_id| commit_projection(&store, commit_id, false))
                    .collect();
                Ok(serde_json::json!({
                    "ok": true,
                    "dangling": dangling,
                    "records": records?,
                    "objects": objects,
                    "links": links,
                }))
            } else {
                Ok(serde_json::json!({
                    "ok": true,
                    "tips": store.state().tips,
                    "objects": objects,
                    "links": links,
                }))
            }
        }
    }
}

fn decode_range_text(bytes: &[u8], encoding: &str) -> Option<String> {
    let encoding = encoding_rs::Encoding::for_label(encoding.trim().as_bytes())?;
    encoding
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(|text| text.into_owned())
}

/// Member file-nodes of `tag`: every file under a dir tagged `tag` (from the
/// live FS) plus any node tagged directly. Unmarked content stays in the
/// denominator — a tag without links is a coverage gap, not a silent pass.
fn tag_member_nodes(
    store: &Store,
    proj_root: &std::path::Path,
    tag: &str,
) -> Result<Vec<(Option<String>, String)>, AppError> {
    let mut members = std::collections::BTreeMap::<String, Option<String>>::new();
    for (node, tags) in &store.state().tags {
        if !tags.contains(tag) {
            continue;
        }
        let Some(path) = omd::relations::node::path_of(store.state(), node) else {
            continue;
        };
        let root_id = node.split_once(':').map(|(_, id)| id).unwrap_or("");
        let statistics = store.read_commit(root_id)?.kind == CommitKind::Import;
        let patterns = if statistics {
            let Some(scope) = import_scope(store, node)? else {
                continue;
            };
            scope
        } else {
            Vec::new()
        };
        let member = if statistics {
            store.file_at_path(path).map(str::to_string)
        } else {
            Some(node.clone())
        };
        members.insert(path.into(), member);
        let dir = proj_root.join(path);
        if dir.is_dir() {
            for relative in omd::sources::scope::resolve(&dir, &patterns).files {
                let child_path = if path.is_empty() {
                    relative.display().to_string()
                } else {
                    format!("{}/{}", path.trim_end_matches('/'), relative.display())
                };
                let child = store.file_at_path(&child_path).map(str::to_string);
                members.insert(child_path, child);
            }
        }
    }
    Ok(members
        .into_iter()
        .map(|(path, node)| (node, path))
        .collect())
}

#[derive(Debug)]
struct FileCoverage {
    unit: omd::relations::range::Mode,
    covered: Option<u64>,
    total: Option<u64>,
    gaps: Vec<omd::relations::range::Range>,
    path: String,
    node: Option<String>,
    source_kind: String,
    source_version_id: Option<String>,
    reason: Option<String>,
}

fn mode_name(mode: omd::relations::range::Mode) -> &'static str {
    match mode {
        omd::relations::range::Mode::Text => "text",
        omd::relations::range::Mode::Byte => "byte",
    }
}

/// Per-file, per-unit link coverage. Current bytes come only from verify's
/// once-collected observations (or a direct read for an untracked file); this
/// query never reruns a command or substitutes historical content.
fn linked_file_coverage(
    store: &Store,
    proj_root: &std::path::Path,
    node: Option<&str>,
    path: &str,
    target_tag: &str,
    observations: &std::collections::BTreeMap<String, pipeline::SuccessfulObservation>,
    dirty: &std::collections::BTreeMap<String, Vec<String>>,
    encoding_override: Option<&str>,
) -> Result<Vec<FileCoverage>, String> {
    if proj_root.join(path).is_dir() {
        return Ok(Vec::new());
    }
    let mut effective = std::collections::BTreeMap::new();
    let mut text_encoding = None;
    let mut has_text = false;
    let mut has_byte = false;
    for child in node
        .and_then(|node| store.state().mounts.get(node))
        .into_iter()
        .flatten()
        .filter(|child| omd::relations::node::is_range_key(child))
    {
        let state = omd::relations::identity::effective_range_state(store, child)
            .map_err(|e| e.to_string())?;
        let Some(range) = state.range else { continue };
        match range.mode {
            omd::relations::range::Mode::Text => has_text = true,
            omd::relations::range::Mode::Byte => has_byte = true,
        }
        let version_id = state
            .source_version_id
            .ok_or_else(|| format!("source version missing for {child}"))?;
        let version = store
            .read_version(&version_id)
            .map_err(|_| format!("version record missing ({version_id})"))?;
        if range.mode == omd::relations::range::Mode::Text {
            if text_encoding.as_ref().is_some_and(|current| {
                version
                    .encoding
                    .as_ref()
                    .is_some_and(|encoding| current != encoding)
            }) {
                return Err("incompatible text encoding views in one file rule".into());
            }
            if text_encoding.is_none() {
                text_encoding = version.encoding;
            }
        }
        effective.insert(child.clone(), (range, version_id));
    }
    if !has_text && !has_byte {
        has_text = true;
    }
    if text_encoding.is_none() {
        text_encoding = node
            .and_then(|node| store.source_version_id(node).ok().flatten())
            .and_then(|version_id| store.read_version(&version_id).ok())
            .and_then(|version| version.encoding);
    }

    let mut linked_text = Vec::new();
    let mut linked_byte = Vec::new();
    for link in store.state().links.values() {
        let Some((range, _)) = effective.get(&link.source) else {
            continue;
        };
        if dirty.contains_key(&link.source) || dirty.contains_key(&link.target) {
            continue;
        }
        let target_parent = omd::relations::node::parent_of(store.state(), &link.target)
            .ok_or_else(|| format!("link target has no current parent: {}", link.target))?;
        if !omd::relations::tags::resolve_tags(store.state(), target_parent).contains(target_tag) {
            continue;
        }
        if omd::relations::node::is_range_key(&link.target)
            && omd::relations::identity::effective_range_state(store, &link.target)
                .map_err(|e| e.to_string())?
                .range
                .is_none()
        {
            continue;
        }
        match range.mode {
            omd::relations::range::Mode::Text => linked_text.push(*range),
            omd::relations::range::Mode::Byte => linked_byte.push(*range),
        }
    }

    let (bytes, source_kind, source_version_id) = if let Some(node) = node {
        let version_id = store.source_version_id(node).map_err(|e| e.to_string())?;
        let source_kind = version_id
            .as_deref()
            .and_then(|id| store.read_version(id).ok())
            .map(|version| version.acquisition.kind().to_string())
            .unwrap_or_else(|| "file".into());
        let Some(observation) = observations.get(node) else {
            let reason = format!("current source unavailable for {node}");
            let mut unavailable = Vec::new();
            if has_text {
                unavailable.push(FileCoverage {
                    unit: omd::relations::range::Mode::Text,
                    covered: None,
                    total: None,
                    gaps: Vec::new(),
                    path: path.into(),
                    node: Some(node.into()),
                    source_kind: source_kind.clone(),
                    source_version_id: version_id.clone(),
                    reason: Some(reason.clone()),
                });
            }
            if has_byte {
                unavailable.push(FileCoverage {
                    unit: omd::relations::range::Mode::Byte,
                    covered: None,
                    total: None,
                    gaps: Vec::new(),
                    path: path.into(),
                    node: Some(node.into()),
                    source_kind,
                    source_version_id: version_id,
                    reason: Some(reason),
                });
            }
            return Ok(unavailable);
        };
        (observation.bytes.clone(), source_kind, version_id)
    } else {
        let full_path = proj_root.join(path);
        if full_path.is_dir() {
            return Ok(Vec::new());
        }
        let bytes = match std::fs::read(&full_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Ok(vec![FileCoverage {
                    unit: omd::relations::range::Mode::Text,
                    covered: None,
                    total: None,
                    gaps: Vec::new(),
                    path: path.into(),
                    node: None,
                    source_kind: "file".into(),
                    source_version_id: None,
                    reason: Some(error.to_string()),
                }]);
            }
        };
        (bytes, "file".into(), None)
    };

    let text_encoding = if has_text {
        let config_cwd = store.config_cwd().unwrap_or(proj_root);
        omd::sources::encoding::resolve_runtime(
            encoding_override,
            text_encoding.as_deref(),
            config_cwd,
            store.root(),
            path,
        )
        .map(Some)
        .map_err(|error| error.to_string())
    } else {
        Ok(None)
    };

    let mut result = Vec::new();
    if has_text {
        match text_encoding {
            Err(reason) => result.push(FileCoverage {
                unit: omd::relations::range::Mode::Text,
                covered: None,
                total: None,
                gaps: Vec::new(),
                path: path.into(),
                node: node.map(str::to_string),
                source_kind: source_kind.clone(),
                source_version_id: source_version_id.clone(),
                reason: Some(reason),
            }),
            Ok(Some(encoding)) => match decode_range_text(&bytes, &encoding) {
                Some(content) => {
                    let len = content.chars().count() as u64;
                    if effective.values().any(|(range, _)| {
                        range.mode == omd::relations::range::Mode::Text && range.end > len
                    }) {
                        result.push(FileCoverage {
                            unit: omd::relations::range::Mode::Text,
                            covered: None,
                            total: Some(len),
                            gaps: Vec::new(),
                            path: path.into(),
                            node: node.map(str::to_string),
                            source_kind: source_kind.clone(),
                            source_version_id: source_version_id.clone(),
                            reason: Some("text range out of bounds for current source".into()),
                        });
                    } else {
                        let stats = omd::relations::coverage::text_coverage(&linked_text, &content);
                        result.push(FileCoverage {
                            unit: omd::relations::range::Mode::Text,
                            covered: Some(stats.covered),
                            total: Some(stats.total),
                            gaps: stats.gaps,
                            path: path.into(),
                            node: node.map(str::to_string),
                            source_kind: source_kind.clone(),
                            source_version_id: source_version_id.clone(),
                            reason: None,
                        });
                    }
                }
                None => result.push(FileCoverage {
                    unit: omd::relations::range::Mode::Text,
                    covered: None,
                    total: None,
                    gaps: Vec::new(),
                    path: path.into(),
                    node: node.map(str::to_string),
                    source_kind: source_kind.clone(),
                    source_version_id: source_version_id.clone(),
                    reason: Some(format!("cannot decode {path} as {encoding}")),
                }),
            },
            Ok(None) => unreachable!(),
        }
    }
    if has_byte {
        let len = bytes.len() as u64;
        let out_of_bounds = effective
            .values()
            .any(|(range, _)| range.mode == omd::relations::range::Mode::Byte && range.end > len);
        if out_of_bounds {
            result.push(FileCoverage {
                unit: omd::relations::range::Mode::Byte,
                covered: None,
                total: Some(len),
                gaps: Vec::new(),
                path: path.into(),
                node: node.map(str::to_string),
                source_kind,
                source_version_id,
                reason: Some("byte range out of bounds for current source".into()),
            });
        } else {
            let stats = omd::relations::coverage::byte_coverage(&linked_byte, len);
            result.push(FileCoverage {
                unit: omd::relations::range::Mode::Byte,
                covered: Some(stats.covered),
                total: Some(stats.total),
                gaps: stats.gaps,
                path: path.into(),
                node: node.map(str::to_string),
                source_kind,
                source_version_id,
                reason: None,
            });
        }
    }
    Ok(result)
}

fn coverage_direction(
    store: &Store,
    proj_root: &std::path::Path,
    source_tag: &str,
    target_tag: &str,
    observations: &std::collections::BTreeMap<String, pipeline::SuccessfulObservation>,
    dirty: &std::collections::BTreeMap<String, Vec<String>>,
    encoding_override: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let mut totals = std::collections::BTreeMap::<String, (u64, u64)>::new();
    let mut incomplete_units = std::collections::BTreeSet::<String>::new();
    let mut files = Vec::new();
    let mut incomplete = false;
    for (node, path) in tag_member_nodes(store, proj_root, source_tag)? {
        match linked_file_coverage(
            store,
            proj_root,
            node.as_deref(),
            &path,
            target_tag,
            observations,
            dirty,
            encoding_override,
        ) {
            Ok(items) => {
                for item in items {
                    let unit = mode_name(item.unit);
                    let status = if item.reason.is_some() {
                        incomplete = true;
                        incomplete_units.insert(unit.into());
                        "incomplete"
                    } else {
                        "complete"
                    };
                    let aggregate = totals.entry(unit.into()).or_default();
                    if let (Some(covered), Some(total)) = (item.covered, item.total) {
                        aggregate.0 += covered;
                        aggregate.1 += total;
                    }
                    let gaps: Vec<_> = item
                        .gaps
                        .iter()
                        .map(|gap| {
                            serde_json::json!({
                                "unit": mode_name(gap.mode),
                                "start": gap.start.to_string(),
                                "end": gap.end.to_string(),
                            })
                        })
                        .collect();
                    files.push(serde_json::json!({
                        "status": status,
                        "store": local_scope(store),
                        "node": item.node.as_deref().map(|node| object_ref_value(store, node)),
                        "commit_id": item.node.as_deref().and_then(|node| store.state().tips.get(node)),
                        "source": {
                            "kind": item.source_kind,
                            "version_id": item.source_version_id,
                        },
                        "project_relative_path": item.path,
                        "unit": unit,
                        "covered": item.covered.map(|value| value.to_string()),
                        "total": item.total.map(|value| value.to_string()),
                        "percentage": match (item.covered, item.total) {
                            (Some(covered), Some(total)) if item.reason.is_none() => {
                                Some(omd::relations::coverage::coverage_percent(covered, total))
                            }
                            _ => None,
                        },
                        "gaps": gaps,
                        "reason": item.reason,
                    }));
                }
            }
            Err(error) => {
                incomplete = true;
                incomplete_units.insert("text".into());
                totals.entry("text".into()).or_default();
                files.push(serde_json::json!({
                    "status": "incomplete",
                    "store": local_scope(store),
                    "node": node.as_deref().map(|node| object_ref_value(store, node)),
                    "commit_id": node.as_deref().and_then(|node| store.state().tips.get(node)),
                    "source": null,
                    "project_relative_path": path,
                    "unit": null,
                    "covered": null,
                    "total": null,
                    "percentage": null,
                    "gaps": [],
                    "reason": error,
                }));
            }
        }
    }
    let groups: Vec<_> = totals
        .into_iter()
        .map(|(unit, (covered, total))| {
            let partial = incomplete_units.contains(&unit);
            serde_json::json!({
                "unit": unit,
                "covered": covered.to_string(),
                "total": total.to_string(),
                "percentage": if partial { None } else { Some(omd::relations::coverage::coverage_percent(covered, total)) },
                "partial": partial,
            })
        })
        .collect();
    let pass = !incomplete
        && groups
            .iter()
            .all(|group| group["covered"] == group["total"]);
    Ok(serde_json::json!({
        "status": if incomplete { "incomplete" } else if pass { "pass" } else { "fail" },
        "groups": groups,
        "files": files,
    }))
}

fn json_requested() -> bool {
    std::env::args_os()
        .skip(1)
        .take_while(|arg| arg != "--")
        .any(|arg| arg == "--json")
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = error.exit_code();
            if exit_code != 0 && json_requested() {
                let out = Envelope::new(
                    false,
                    serde_json::Value::Null,
                    vec![Diagnostic::new("usage", "error", error.to_string())],
                );
                println!("{}", serde_json::to_string(&out).unwrap());
            } else {
                let _ = error.print();
            }
            return ExitCode::from(exit_code as u8);
        }
    };
    match run(&cli) {
        Ok(v) => {
            let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(true);
            let diagnostics = diagnostics_from_data(&v);
            emit(
                &cli,
                serde_json::to_value(Envelope::new(ok, v, diagnostics)).unwrap(),
            );
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            let diagnostic = Diagnostic::new(e.kind.diagnostic(), "error", e.message.clone());
            let out = Envelope::new(
                false,
                e.data.unwrap_or(serde_json::Value::Null),
                vec![diagnostic],
            );
            let out = serde_json::to_value(out).unwrap();
            if cli.json {
                println!("{}", serde_json::to_string(&out).unwrap());
            } else {
                eprintln!("error: {}", serde_json::to_string_pretty(&out).unwrap());
            }
            ExitCode::from(e.kind.exit_code())
        }
    }
}
