//! `omd` — command-line entry to the OMD tracking core.
//!
//! Command surface follows the spec's commit-verb alias model: mutating
//! operations go through `omd commit <kind>`; top-level verbs like `init`,
//! `import`, `remove`, `delete`, `rename`, `copy` are aliases into those
//! commit kinds. `--json` selects a machine-readable envelope on every verb.

use clap::{Parser, Subcommand};

use omd::records::commit::CommitKind;
use omd::records::pipeline;
use omd::records::store::{Expected, NoProbe, Store};
use omd::records::time::{OsRng, SystemClock};

use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "omd", version, about = "Object-tracking metadata store")]
struct Cli {
    /// Emit a JSON envelope on stdout (never mixed with stderr).
    #[arg(long, global = true)]
    json: bool,

    /// Explicit metadata directory (overrides discovery).
    #[arg(long, global = true, value_name = "DIR")]
    meta: Option<PathBuf>,

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

    /// Acquisition source-ref for init/commit (`--source-ref`).
    /// `command::<exe>::<JSON argv>` runs the command and records its stdout
    /// as the version's `Acquisition::Command` (vs a file read). Only when
    /// the flag is given does the command run — never on config load.
    #[arg(long, global = true)]
    source_ref: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
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
        #[arg(long)]
        range: Option<String>,
        #[arg(long)]
        timestamp: Option<String>,
        /// `commit reset <path> --target <commit-id>` — the commit to reset to.
        #[arg(long)]
        reset_target: Option<String>,
        /// `commit link --source A@.. --target B@..`
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        target: Option<String>,
        /// `commit adapt --link-id L --changes c1,c2 --reason r`
        #[arg(long)]
        link_id: Option<String>,
        #[arg(long)]
        changes: Option<String>,
        /// `commit adapt --stop` — source-side branch stop after handling.
        #[arg(long)]
        stop: bool,
        /// `--adapt '<JSON {link_id,changes,reason}>'` — spec's repeatable
        /// adapt-object form (alternative to the separate flags).
        #[arg(long)]
        adapt: Vec<String>,
        /// `commit clean --no-reason` — explicitly omit the stop reason.
        #[arg(long = "no-reason")]
        no_reason: bool,
        /// `commit ... --link-from R` — create R→this link in the block.
        #[arg(long = "link-from")]
        link_from: Vec<String>,
        /// `commit ... --link-to R` — create this→R link in the block.
        #[arg(long = "link-to")]
        link_to: Vec<String>,
        /// `commit xlink <path> --source <local-range> --target peer:<store>:<file>@<range>`
        /// — a cross-store link: local source range → range in a registered
        /// peer store (target resolved read-only, no metadata merge).
        #[arg(long = "xlink-to")]
        xlink_to: Vec<String>,
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
    /// `omd replace <commit-id> --source <ref>` — rebind the commit's
    /// recorded full version to a new source with identical content.
    Replace {
        commit_id: String,
        /// `file:<path>` | `git::<JSON>` | `command::<exe>::<args>`.
        #[arg(long)]
        source: String,
    },
    /// `omd register <peer-store-id> <locator>` — register a peer store.
    Register { store_id: String, locator: String },
    /// `omd protect <target> --peer <peer-store-id> --record <id>` — persist
    /// an inbound protection credential before the peer's record publishes.
    Protect {
        target: String,
        #[arg(long)]
        peer: String,
        #[arg(long)]
        record: String,
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

/// Walk a node chain tip→root via previous_id links (tip-first order).
fn log_chain(store: &Store, root: &Path, node_or_commit: &str) -> Vec<String> {
    // Accept a node key (walk its tip) OR a commit id (walk that commit).
    let start = store
        .state()
        .tips
        .get(node_or_commit)
        .cloned()
        .unwrap_or_else(|| node_or_commit.to_string());
    let mut out = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    let mut cur = start;
    while !cur.is_empty() && visited.insert(cur.clone()) {
        out.push(cur.clone());
        cur = commit_prev(root, &cur).unwrap_or_default();
    }
    out
}

/// Render the mount tree as nested JSON from `start` (or the implicit root).
/// Tree shows mount hierarchy: root → file nodes → range children. Only
/// mounted nodes expand — never unpublished material as history.
/// Metadata directory resolution: explicit `--meta` > `OMD_META` env >
/// nearest ancestor `.omd/` (walk up parents) > `./.omd` default. Never a
/// full-repo recursive scan; ambiguity among single-level candidates is an
/// error, never a silent pick.
fn resolve_meta_dir(cli: &Cli) -> PathBuf {
    if let Some(m) = &cli.meta {
        return m.clone();
    }
    if let Ok(m) = std::env::var("OMD_META")
        && !m.is_empty()
    {
        return PathBuf::from(m);
    }
    // Walk ancestors for an existing `.omd/` — running from a subdirectory
    // finds the project store, never fabricates a new one per dir.
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let cand = dir.join(".omd");
        if cand.is_dir() {
            return cand;
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from(".omd")
}

/// Resolve a link endpoint arg to its canonical `range:` key.
/// Accepts `file@mode:s-e`, `range:file@mode:s-e`, or a bare range key.
/// Non-range args pass through verbatim (they'll fail the range check).
fn resolve_range_key(r: &str) -> String {
    let bare = r.strip_prefix("range:").unwrap_or(r);
    if let Some(at) = bare.find('@') {
        let (path, span) = bare.split_at(at);
        if let Some((mode, s, e)) = omd::relations::node::parse_range_arg(&span[1..]) {
            return omd::relations::node::range_key(path, mode, s, e);
        }
    }
    r.to_string()
}

/// Parse a cross-store link endpoint `peer:<store_id>:<file>@<range>` into
/// (peer_store_id, canonical range key). Returns None when unparseable.
fn parse_peer_endpoint(r: &str) -> Option<(String, String)> {
    let rest = r.strip_prefix("peer:")?;
    let (sid, range_part) = rest.split_once(':')?;
    if sid.is_empty() {
        return None;
    }
    let key = resolve_range_key(range_part);
    if omd::relations::node::is_range_key(&key) {
        Some((sid.to_string(), key))
    } else {
        None
    }
}

fn mount_tree(
    store: &Store,
    start: Option<&str>,
    max_depth: usize,
    file_only: bool,
) -> serde_json::Value {
    let mounts = &store.state().mounts;
    let tips = &store.state().tips;
    // Build node→children from mounts; also synthesize root→file mounts for
    // tips not already mounted under another parent.
    fn build(
        node: &str,
        mounts: &std::collections::BTreeMap<String, Vec<String>>,
        tips: &std::collections::BTreeMap<String, String>,
        depth: usize,
        max_depth: usize,
        file_only: bool,
    ) -> serde_json::Value {
        if depth > max_depth {
            return serde_json::json!({ "node": node, "truncated": true });
        }
        let mut children: Vec<serde_json::Value> = mounts
            .get(node)
            .map(|c| {
                c.iter()
                    .filter(|ch| !(file_only && ch.starts_with("range:")))
                    .map(|ch| build(ch, mounts, tips, depth + 1, max_depth, file_only))
                    .collect()
            })
            .unwrap_or_default();
        // Range children of this node — only when not file-only, and not
        // already listed via mounts (dedup).
        if !file_only {
            let prefix = format!("range:{}", node.strip_prefix("file:").unwrap_or(node));
            let already: std::collections::BTreeSet<String> = mounts
                .get(node)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            for k in tips.keys() {
                if !already.contains(k)
                    && (k.starts_with(&prefix) || k.starts_with(&format!("range:{}@", node)))
                {
                    children.push(serde_json::json!({ "node": k, "children": [] }));
                }
            }
        }
        serde_json::json!({ "node": node, "children": children })
    }
    // Root: every file: tip not already a mount child.
    let mounted: std::collections::BTreeSet<String> = mounts.values().flatten().cloned().collect();
    let mut root_children: Vec<serde_json::Value> = Vec::new();
    for node in tips.keys() {
        if node.starts_with("file:") && !mounted.contains(node) {
            root_children.push(build(node, mounts, tips, 1, max_depth, file_only));
        }
    }
    let root = start.unwrap_or("root");
    if root == "root" {
        serde_json::json!({ "ok": true, "tree": { "node": "root", "children": root_children } })
    } else {
        serde_json::json!({ "ok": true, "tree": build(root, mounts, tips, 0, max_depth, file_only) })
    }
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

fn meta_root(cli: &Cli) -> PathBuf {
    resolve_meta_dir(cli)
}

/// Wrap a command payload in the contract envelope `{schema_version, ok,
/// data, diagnostics}`. `data` carries the command result; `diagnostics`
/// carries structured `{kind, severity, message, store, node, commit_id}`
/// entries (nulls for inapplicable fields). stdout is JSON only — progress
/// and source stderr never merge into the JSON channel.
fn envelope(
    ok: bool,
    data: serde_json::Value,
    diagnostics: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "1",
        "ok": ok,
        "data": data,
        "diagnostics": diagnostics,
    })
}

fn emit(cli: &Cli, v: serde_json::Value) {
    if cli.json {
        println!("{}", serde_json::to_string(&v).unwrap());
    } else {
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    }
}

fn run(cli: &Cli) -> Result<serde_json::Value, String> {
    let root = meta_root(cli);
    match &cli.cmd {
        Cmd::Delete { path } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let cid = pipeline::commit_lifecycle(
                &mut store,
                &mut NoProbe,
                &OsRng,
                &SystemClock,
                CommitKind::Delete,
                path,
                None,
                "",
                &Expected::default(),
            )
            .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": "Delete" }))
        }
        Cmd::Init { path } | Cmd::Import { path, .. } | Cmd::Remove { path } => {
            let kind = match &cli.cmd {
                Cmd::Init { .. } => CommitKind::Init,
                Cmd::Import { .. } => CommitKind::Import,
                Cmd::Remove { .. } => CommitKind::Remove,
                _ => unreachable!(),
            };
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let p = Path::new(path);
            let node = format!("file:{path}");
            // init is not stackable: a second init on an already-tracked
            // path is a usage error, never an appended record (the init
            // records first identity, nothing to chain onto).
            if matches!(&cli.cmd, Cmd::Init { .. }) && store.state().tips.contains_key(&node) {
                return Err(format!(
                    "init refused: {node} is already tracked (init is not stackable)"
                ));
            }
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
            // A directory import/remove records the statistics scope (no file
            // bytes to observe); a file path is an ordinary tracked object.
            let cid = if p.is_dir() {
                pipeline::commit_marker(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    &SystemClock,
                    &node,
                    kind,
                    payload,
                    &Expected::default(),
                )
                .map_err(|e| e.to_string())?
            } else {
                let enc =
                    omd::sources::encoding::resolve(&omd::sources::encoding::EncodingChoice {
                        cli: cli.encoding.clone(),
                        recorded: None,
                        file_config: None,
                        project_default: None,
                        user_default: None,
                    });
                pipeline::commit_file(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    &SystemClock,
                    &node,
                    p,
                    kind,
                    payload,
                    &Expected::default(),
                    Some(&enc),
                )
                .map_err(|e| e.to_string())?
            };
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") }))
        }
        Cmd::Commit {
            kind,
            path,
            reason,
            range,
            timestamp,
            reset_target,
            source,
            target,
            link_id,
            changes,
            stop,
            adapt,
            no_reason,
            link_from,
            link_to,
            xlink_to,
            id,
            tag,
            rule,
            level,
            skip,
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
                other => return Err(format!("unknown commit kind: {other}")),
            };
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            // An unactivated copy (store dir copied, not re-registered) must
            // not publish business writes — history/diagnostic reads only.
            if !omd::records::cross::activated(store.state()) {
                return Err("store is an unregistered copy: business writes refused until `omd activate` registers a new store_id".into());
            }
            // 8.4 pre-check: same-direction duplicate *resolved* range in
            // one command rejects the whole command before any write — never
            // silently dedup or create two identical links.
            {
                // Duplicate detection is by RESOLVED range identity (spec:
                // "解析后的 range 身份，而非参数文本") — different spellings
                // of the same range (`a.md@0-1`, `range:a.md@text:0-1`)
                // resolve to the same canonical key and are duplicates.
                // Opposite directions are NOT duplicates.
                // Resolve range shorthand (file@mode:s-e / range:file@mode:s-e)
                // to the canonical `range:` key for dedup + link endpoints.
                let resolve_range = resolve_range_key;
                let mut seen_from = std::collections::HashSet::new();
                for r in link_from {
                    if !seen_from.insert(resolve_range(r)) {
                        return Err(format!("duplicate --link-from range in one command: {r}"));
                    }
                }
                let mut seen_to = std::collections::HashSet::new();
                for r in link_to {
                    if !seen_to.insert(resolve_range(r)) {
                        return Err(format!("duplicate --link-to range in one command: {r}"));
                    }
                }
            }
            // Resolve the node: --range targets a first-class range chain
            // mounted under the file; bare path targets the file chain.
            let node = match (range, id) {
                // --id names a commit in the target range chain — resolve it
                // to the chain's node key. A tip id resolves directly; a
                // non-tip commit id resolves by walking each tip's
                // previous_id chain until the commit is found.
                (_, Some(cid)) => {
                    let direct = store
                        .state()
                        .tips
                        .iter()
                        .find(|(_, tip)| *tip == cid)
                        .map(|(n, _)| n.clone());
                    match direct {
                        Some(n) => n,
                        None => {
                            // Walk every tip's chain for the commit id.
                            let mut found = None;
                            'outer: for (node, tip) in &store.state().tips {
                                let mut cur = tip.clone();
                                let mut visited = std::collections::BTreeSet::new();
                                while !cur.is_empty() && visited.insert(cur.clone()) {
                                    if cur == *cid {
                                        found = Some(node.clone());
                                        break 'outer;
                                    }
                                    cur = commit_prev(&root, &cur).unwrap_or_default();
                                }
                            }
                            found.ok_or_else(|| format!("--id: no chain contains commit {cid}"))?
                        }
                    }
                }
                // New independent range over identical coords gets a nonce.
                (Some(r), None) => {
                    let (mode, s, e) = omd::relations::node::parse_range_arg(r)
                        .ok_or_else(|| format!("bad --range: {r}"))?;
                    let base = omd::relations::node::range_key(path, mode, s, e);
                    if store.state().tips.contains_key(&base) {
                        // Same coords, no --id: a NEW independent object.
                        let nonce = format!(
                            "{:x}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos()
                        );
                        omd::relations::node::range_key_nonce(path, mode, s, e, &nonce)
                    } else {
                        base
                    }
                }
                (None, None) => format!("file:{path}"),
            };
            let mut reset_outcome: Option<serde_json::Value> = None;
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            if let Some(r) = reason {
                payload.insert("reason".into(), r.clone().into());
            }
            if let Some(r) = range {
                payload.insert("range".into(), r.clone().into());
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
                    omd::records::time::parse_rfc3339(t).ok_or("bad --timestamp: need RFC3339")
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
                    | CommitKind::Tag
            );
            // --link-from/--link-to wrap the commit in an ATOMIC block: the
            // range commit + each link creation are recorded inside one
            // BEGIN/END on this node's chain (spec 8.4 atomic combination).
            let combo = !link_from.is_empty() || !link_to.is_empty();
            if combo {
                let mut pl = serde_json::Map::new();
                pl.insert("path".into(), path.clone().into());
                pipeline::commit_marker(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    clock,
                    &node,
                    CommitKind::AtomicBegin,
                    pl,
                    &Expected::default(),
                )
                .map_err(|e| e.to_string())?;
            }
            let cid = if kind == CommitKind::Link {
                let src = source.clone().unwrap_or_default();
                let tgt = target.clone().unwrap_or_default();
                let r = reason.clone().unwrap_or_default();
                pipeline::commit_link(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    clock,
                    &node,
                    &src,
                    &tgt,
                    &r,
                    &Expected::default(),
                )
                .map_err(|e| e.to_string())?
            } else if kind == CommitKind::Adapt {
                // `--adapt '<JSON {link_id,changes,reason}>'` is the spec's
                // repeatable object form; each entry runs one adapt. The
                // flat flags (--link-id/--changes/--reason) run a single
                // adapt — both map to the same commit_adapt path.
                if !adapt.is_empty() {
                    let mut last = String::new();
                    for entry in adapt {
                        let v: serde_json::Value = serde_json::from_str(entry)
                            .map_err(|_| format!("bad --adapt JSON: {entry}"))?;
                        let lid = v
                            .get("link_id")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let ch: Vec<String> = v
                            .get("changes")
                            .and_then(|x| x.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|s| s.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let r = v
                            .get("reason")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        last = pipeline::commit_adapt(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &lid,
                            &ch,
                            &r,
                            *stop,
                            &Expected::default(),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    last
                } else {
                    let lid = link_id.clone().unwrap_or_default();
                    let ch: Vec<String> = changes
                        .clone()
                        .unwrap_or_default()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let r = reason.clone().unwrap_or_default();
                    pipeline::commit_adapt(
                        &mut store,
                        &mut NoProbe,
                        &OsRng,
                        clock,
                        &node,
                        &lid,
                        &ch,
                        &r,
                        *stop,
                        &Expected::default(),
                    )
                    .map_err(|e| e.to_string())?
                }
            } else if kind == CommitKind::Reset {
                // reset resolves the target's kind+prev from its on-disk record.
                // The target is an explicit --target <commit-id> — never a
                // reason overload. `--reason` on reset is a usage error.
                if reason.is_some() {
                    return Err("reset takes --reset-target <commit-id>, not --reason".into());
                }
                let target = reset_target
                    .clone()
                    .unwrap_or_else(|| range.clone().unwrap_or_default());
                // The reset operates on the node whose chain CONTAINS the
                // target commit — resolve it by walking each tip's
                // previous_id chain, not the file node.
                let reset_node = {
                    let mut found = node.clone();
                    for (n, tip) in &store.state().tips {
                        let mut cur = tip.clone();
                        let mut guard = 0usize;
                        while !cur.is_empty() && guard < 100_000 {
                            if cur == target {
                                found = n.clone();
                                break;
                            }
                            cur = commit_prev(&root, &cur).unwrap_or_default();
                            guard += 1;
                        }
                        if found != node {
                            break;
                        }
                    }
                    found
                };
                // Cross-chain interior check: a range commit created while
                // its parent FILE's block was open carries an `in_block`
                // payload stamp (commit_file records the open BEGIN id) —
                // the file chain's BEGIN is invisible from the range's own
                // ancestor walk, so membership is read from the stamp.
                if omd::relations::node::is_range_key(&reset_node) {
                    let stamped =
                        std::fs::read_to_string(root.join(format!("commits/{target}.toml")))
                            .ok()
                            .and_then(|s| toml::from_str::<omd::records::commit::Commit>(&s).ok())
                            .and_then(|c| {
                                c.payload
                                    .get("in_block")
                                    .and_then(|v| v.as_str().map(String::from))
                            });
                    if stamped.is_some() {
                        return Err(format!(
                            "reset target is an ordinary block member: {target}"
                        ));
                    }
                }
                let mut lk = |id: &str| -> Option<(CommitKind, String)> {
                    let s =
                        std::fs::read_to_string(root.join(format!("commits/{id}.toml"))).ok()?;
                    let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
                    Some((c.kind, c.previous_id))
                };
                match pipeline::reset(&reset_node, &target, &mut lk) {
                    Ok(out) => {
                        // Apply the reset to state FIRST: move tip to the
                        // landing point, dangle removed commits, withdraw
                        // link/adapt records created in the removed segment.
                        // Then the reset marker commit chains onto `actual`.
                        let st = store.state().clone();
                        let mut lk2 = |id: &str| -> Option<(CommitKind, String)> {
                            let s =
                                std::fs::read_to_string(root.join(format!("commits/{id}.toml")))
                                    .ok()?;
                            let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
                            Some((c.kind, c.previous_id))
                        };
                        // The reset records its own marker ON the landing
                        // point — set the tip to `actual` before the marker
                        // so the marker chains onto it and becomes the tip.
                        let mut pre = st.clone();
                        pipeline::apply_reset_to_state(
                            &mut pre,
                            &reset_node,
                            &out.requested,
                            &out.actual,
                            &mut lk2,
                        );
                        store.set_state(pre).map_err(|e| e.to_string())?;
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
                            &Expected::default(),
                        )
                        .map_err(|e| e.to_string())?;
                        // Surface the reset outcome — boundary resets carry a
                        // machine-readable warning + requested/actual ids.
                        reset_outcome = Some(serde_json::json!({
                            "requested": out.requested,
                            "actual": out.actual,
                            "warning": out.warning,
                        }));
                        cid
                    }
                    Err(e) => return Err(e.to_string()),
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
                    &Expected::default(),
                )
                .map_err(|e| e.to_string())?
            } else {
                // commit verify <file>: a file hash commit MUST NOT pass while
                // any child range still carries an unhandled obligation or a
                // dirty mark — the new hash cannot hide a range's debt (4.5).
                if kind == CommitKind::FileVerify {
                    for (k, ds) in &store.state().dirty {
                        if omd::relations::node::parent_of(k) == node
                            && (!ds.obligations.is_empty() || !ds.dirty.is_empty())
                        {
                            return Err(format!(
                                "verify blocked: range {k} still has unhandled obligations/dirty"
                            ));
                        }
                    }
                    // Outstanding (not-yet-persisted) range work also blocks:
                    // re-run the Myers dirty check on each child range tip —
                    // an uncommitted edit that would mark the range dirty
                    // prevents a clean file-verify from hiding it (4.5).
                    for (k, tip) in &store.state().tips {
                        if omd::relations::node::is_range_key(k)
                            && omd::relations::node::parent_of(k) == node
                            && pipeline::range_needs_review(&store, tip)
                        {
                            return Err(format!(
                                "verify blocked: range {k} has uncommitted changes needing review"
                            ));
                        }
                    }
                }
                // Command source: `--source-ref 'command::<exe>::<args>'`
                // runs the command and records Acquisition::Command.
                if let Some(sr) = &cli.source_ref {
                    if let Ok((exe, argv)) = omd::sources::command::parse_command_ref(sr) {
                        let proj = std::env::current_dir().map_err(|e| e.to_string())?;
                        pipeline::commit_command_source(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &exe,
                            &argv,
                            &proj,
                            kind,
                            payload.clone(),
                            &Expected::default(),
                        )
                        .map_err(|e| e.to_string())?
                    } else {
                        return Err(format!("--source-ref not a command ref: {sr}"));
                    }
                } else {
                    // Resolve encoding: --encoding flag > recorded > file cfg
                    // > project default > user default > UTF-8.
                    let enc =
                        omd::sources::encoding::resolve(&omd::sources::encoding::EncodingChoice {
                            cli: cli.encoding.clone(),
                            recorded: None,
                            file_config: None,
                            project_default: None,
                            user_default: None,
                        });
                    pipeline::commit_file(
                        &mut store,
                        &mut NoProbe,
                        &OsRng,
                        clock,
                        &node,
                        Path::new(path),
                        kind,
                        payload,
                        &Expected::default(),
                        Some(&enc),
                    )
                    .map_err(|e| e.to_string())?
                }
            };
            // Create the requested links inside the block, then close it.
            // --link-from/--link-to connect RANGE nodes: the committing node
            // must itself be a range (use --range on this commit), and each
            // endpoint arg resolves to its canonical `range:` key.
            if combo {
                if !omd::relations::node::is_range_key(&node) {
                    return Err("--link-from/--link-to require --range: links connect ranges, not whole files".into());
                }
                // Track succeeded member link-ids + operation id so a mid-block
                // failure reports WHICH members published, the failed step, and
                // the still-open block boundary — never a silent partial commit.
                let mut ok_members: Vec<String> = Vec::new();
                let operation_id = {
                    let mut idb = [0u8; 16];
                    omd::testing::Rng::fill(&OsRng, &mut idb);
                    hex::encode(idb)
                };
                let open_block = store
                    .state()
                    .open_blocks
                    .get(&node)
                    .cloned()
                    .unwrap_or_default();
                let mut run_member =
                    |src: String, tgt: String, dir: &str| -> Result<String, String> {
                        pipeline::commit_link(
                            &mut store,
                            &mut NoProbe,
                            &OsRng,
                            clock,
                            &node,
                            &src,
                            &tgt,
                            "combo",
                            &Expected::default(),
                        )
                        .map_err(|e| format!("{dir}:{e}"))
                    };
                for r in link_from {
                    let src = resolve_range_key(r);
                    match run_member(src.clone(), node.clone(), "from") {
                        Ok(lid) => ok_members.push(lid),
                        Err(e) => {
                            return Err(serde_json::json!({
                                "ok": false, "kind": "combo_partial_failure",
                                "succeeded_members": ok_members,
                                "failed_step": format!("link-from {src}"),
                                "open_block": open_block,
                                "operation_id": operation_id,
                                "error": e,
                            })
                            .to_string());
                        }
                    }
                }
                for r in link_to {
                    let tgt = resolve_range_key(r);
                    match run_member(node.clone(), tgt.clone(), "to") {
                        Ok(lid) => ok_members.push(lid),
                        Err(e) => {
                            return Err(serde_json::json!({
                                "ok": false, "kind": "combo_partial_failure",
                                "succeeded_members": ok_members,
                                "failed_step": format!("link-to {tgt}"),
                                "open_block": open_block,
                                "operation_id": operation_id,
                                "error": e,
                            })
                            .to_string());
                        }
                    }
                }
                let mut pl = serde_json::Map::new();
                pl.insert("path".into(), path.clone().into());
                pipeline::commit_marker(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    clock,
                    &node,
                    CommitKind::AtomicEnd,
                    pl,
                    &Expected::default(),
                )
                .map_err(|e| e.to_string())?;
            }
            // Cross-store links: `--xlink-to peer:<store_id>:<file>@<range>`.
            // A records a link pointing at a range in a registered peer store
            // B; B gets a symmetric inbound credential so the target stays
            // queryable + gc-protected. Never a metadata merge — each store
            // keeps its own .omd/commits/tips.
            let mut xlink_ids: Vec<String> = Vec::new();
            for r in xlink_to {
                let (psid, ptarget) = parse_peer_endpoint(r).ok_or_else(|| {
                    format!("bad --xlink-to endpoint (want peer:<store>:<file>@<range>): {r}")
                })?;
                let peer_locator = store
                    .state()
                    .peers
                    .get(&psid)
                    .map(|p| p.locator.clone())
                    .ok_or_else(|| format!("peer store not registered: {psid}"))?;
                // Persist B's inbound credential FIRST (protects the target
                // before A's link record publishes) — conservative retention.
                let mut lid_b = [0u8; 8];
                omd::testing::Rng::fill(&OsRng, &mut lid_b);
                let pending_link_id = format!("xlink-{}", hex::encode(lid_b));
                {
                    let peer_root = std::path::Path::new(&peer_locator);
                    let mut peer_store = Store::open(peer_root)
                        .map_err(|e| format!("peer store unreadable at {peer_locator}: {e}"))?;
                    // Target range must exist in the peer's live tips.
                    if !peer_store.state().tips.contains_key(&ptarget) {
                        return Err(format!(
                            "peer target range does not exist: {ptarget} @ {psid}"
                        ));
                    }
                    let mut pst = peer_store.state().clone();
                    omd::records::cross::persist_inbound(
                        &mut pst,
                        &store.state().store_id,
                        &pending_link_id,
                        &ptarget,
                    );
                    peer_store.set_state(pst).map_err(|e| e.to_string())?;
                }
                // Now A publishes its link record.
                let link_id = pipeline::commit_xlink(
                    &mut store,
                    &mut NoProbe,
                    &OsRng,
                    clock,
                    &node,
                    &node,
                    &psid,
                    &ptarget,
                    reason.as_deref().unwrap_or("xlink"),
                    &Expected::default(),
                    |loc| {
                        Store::open(std::path::Path::new(loc))
                            .ok()
                            .map(|s| s.state().tips.clone())
                    },
                )
                .map_err(|e| e.to_string())?;
                xlink_ids.push(link_id);
            }
            let mut out =
                serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") });
            if !xlink_ids.is_empty() {
                out.as_object_mut()
                    .unwrap()
                    .insert("xlinks".into(), xlink_ids.into());
            }
            if let Some(ro) = reset_outcome {
                out.as_object_mut().unwrap().insert("reset".into(), ro);
            }
            Ok(out)
        }
        Cmd::Verify { .. } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            let run_cmd = omd::sources::permission::may_run(&omd::sources::permission::RunChoice {
                cli: cli.run_command,
                config: None,
            });
            let rep = pipeline::verify(&store, run_cmd);
            Ok(serde_json::to_value(rep).unwrap())
        }
        Cmd::Check { path } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            // Resolve every recorded Import scope against the live FS, then
            // report which in-scope files carry unexpired confirmed coverage.
            // New members auto-enter statistics because scope re-resolves now.
            let mut scopes: Vec<(String, Vec<String>)> = Vec::new();
            if let Ok(rd) = std::fs::read_dir(root.join("commits")) {
                for e in rd.flatten() {
                    if let Ok(txt) = std::fs::read_to_string(e.path())
                        && let Ok(c) = toml::from_str::<omd::records::commit::Commit>(&txt)
                        && c.kind == omd::records::commit::CommitKind::Import
                    {
                        let path = c
                            .payload
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let scope: Vec<String> = c
                            .payload
                            .get("scope")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        scopes.push((path, scope));
                    }
                }
            }
            let mut report = serde_json::Map::new();
            let mut all_files = Vec::new();
            let proj_root = std::env::current_dir().unwrap_or_default();
            // A dir-import enters members into the statistics SCOPE (the
            // denominator) — it does NOT confer confirmed content coverage.
            // `tracked` = the file has its own confirmed tracked node; scope
            // membership only brings it into the check's denominator.
            for (dir, pats) in &scopes {
                let base = if dir.is_empty() {
                    proj_root.clone()
                } else {
                    proj_root.join(dir)
                };
                let scope = omd::sources::scope::resolve(&base, pats);
                for f in &scope.files {
                    let by_name = format!("file:{}", f.display());
                    let by_scope = format!("file:{}/{}", dir, f.display());
                    // Confirmed coverage needs a *tracked node with confirmed
                    // content* — an unconfirmed member is a coverage gap.
                    let tracked = store.state().tips.contains_key(&by_name)
                        || store.state().tips.contains_key(&by_scope);
                    all_files.push(serde_json::json!({
                        "file": f.display().to_string(),
                        "scope": dir,
                        "tracked": tracked,
                        "in_scope": true,
                    }));
                }
                if !scope.problems.is_empty() {
                    report.insert("problems".into(), scope.problems.clone().into());
                }
            }
            // A tracked file node whose source vanished reports `incomplete`
            // — never a silent empty `files:[]` success. This is check's own
            // honesty floor, independent of `verify`'s `missing` diagnostic.
            let mut any_missing = false;
            for (k, tip) in &store.state().tips {
                if let Some(p) = k.strip_prefix("file:") {
                    let is_tombstone = store
                        .read_commit(tip)
                        .map(|c| c.kind == omd::records::commit::CommitKind::Delete)
                        .unwrap_or(false);
                    if !is_tombstone && !proj_root.join(p).exists() {
                        all_files.push(serde_json::json!({
                            "file": p, "tracked": true, "in_scope": true,
                            "status": "incomplete", "reason": "source missing",
                        }));
                        any_missing = true;
                    }
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
                // Coverage of src_tag members by links targeting tgt_tag.
                // Member files come from the tagged dir's live FS listing —
                // unmarked content stays in the denominator (a tag without
                // links is a gap, not a silent pass).
                let cov = |s: &str, t: &str| -> (u64, u64) {
                    let mut covered = 0u64;
                    let mut denom = 0u64;
                    for node in tag_member_nodes(&store, &proj_root, s) {
                        let linked = count_linked_positions(&store, &node, t);
                        let total = file_denom(&proj_root, &node);
                        covered += linked.min(total);
                        denom += total;
                    }
                    (covered, denom)
                };
                let (fc, fd) = cov(src_tag, tgt_tag);
                // A member whose content can't be read contributes to the
                // denominator but its covered positions are unknown → the
                // item is `incomplete`, never a fabricated 0%/100%.
                let mut any_unreadable = false;
                for node in tag_member_nodes(&store, &proj_root, src_tag) {
                    let p = node.strip_prefix("file:").unwrap_or(&node);
                    if !proj_root.join(p).exists() {
                        any_unreadable = true;
                    }
                }
                let forward_ok = fd == 0 || fc >= fd;
                let mut pass = forward_ok;
                if two_way {
                    let (rc, rd) = cov(tgt_tag, src_tag);
                    pass = pass && (rd == 0 || rc >= rd);
                }
                let status = if rule.skip {
                    "skipped"
                } else if any_unreadable {
                    "incomplete"
                } else if pass {
                    "pass"
                } else {
                    "fail"
                };
                if !rule.skip && !any_unreadable && !pass && rule.level == "fail" {
                    check_failed = true;
                }
                rules_out.push(serde_json::json!({
                    "rule": name, "level": rule.level, "status": status,
                    "unit": "position",
                    "covered": fc, "total": fd,
                    "coverage": if fd == 0 || any_unreadable { serde_json::Value::Null } else { serde_json::json!(fc * 100 / fd) },
                }));
            }
            report.insert("rules".into(), rules_out.into());
            if let Some(p) = path {
                report.insert("query".into(), p.clone().into());
            }
            Ok(serde_json::json!({ "ok": !check_failed, "check": report }))
        }
        Cmd::Rename { source, target } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let cid = pipeline::commit_lifecycle(
                &mut store,
                &mut NoProbe,
                &OsRng,
                &SystemClock,
                CommitKind::Rename,
                source,
                Some(target),
                "",
                &Expected::default(),
            )
            .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": "Rename" }))
        }
        Cmd::Copy { source, target } => {
            // copy = a fresh init on the target path — new identity, source
            // untouched. Target file must exist to observe.
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let node = format!("file:{target}");
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), target.clone().into());
            let cid = pipeline::commit_file(
                &mut store,
                &mut NoProbe,
                &OsRng,
                &SystemClock,
                &node,
                Path::new(target),
                CommitKind::Init,
                payload,
                &Expected::default(),
                None,
            )
            .map_err(|e| e.to_string())?;
            Ok(
                serde_json::json!({ "ok": true, "commit": cid, "kind": "Init", "copied_from": source }),
            )
        }
        Cmd::Note {
            action,
            commit_id,
            target,
            text,
        } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            // Seq is max-of-notes+1 — two concurrent writers must serialize
            // or they'd compute the same seq. Lock around the read+append.
            store.lock().map_err(|e| e.to_string())?;
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
                    let nid =
                        omd::records::notes::append(&root, &note).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "ok": true, "note": nid, "kind": action }))
                }
                "list" => {
                    let notes = omd::records::notes::list_for(&root, commit_id);
                    Ok(serde_json::json!({ "ok": true, "commit_id": commit_id, "notes": notes }))
                }
                other => Err(format!("unknown note action: {other}")),
            }
        }
        Cmd::Replace { commit_id, source } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            // Resolve the commit's recorded version + its complete content.
            let commit = store.read_commit(commit_id).map_err(|e| e.to_string())?;
            let version = store
                .read_version(&commit.content_ref)
                .map_err(|e| e.to_string())?;
            let recorded = store
                .read_content(&version.sha256)
                .map_err(|e| e.to_string())?;
            // Read the new source's *complete* bytes per its kind.
            // A bare existing path is a file source even without `proj:`.
            let new_bytes = if std::path::Path::new(source).exists() {
                std::fs::read(source).map_err(|e| format!("read {source}: {e}"))?
            } else {
                match omd::sources::reference::parse_source_ref(source) {
                    Ok(omd::sources::reference::SourceRef::File { path, .. }) => {
                        std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?
                    }
                    Ok(omd::sources::reference::SourceRef::Git { repo, commit, path }) => {
                        let g = omd::sources::git::GitRef {
                            repo: repo.into(),
                            commit,
                            path,
                        };
                        omd::sources::git::read_blob(&g)
                            .map_err(|_| "git source unobtainable".to_string())?
                    }
                    Ok(omd::sources::reference::SourceRef::Command { executable, args }) => {
                        let out = omd::sources::command::observe_command(
                            &executable,
                            &args,
                            &std::env::current_dir().unwrap_or_default(),
                        )
                        .map_err(|_| "command failed".to_string())?;
                        out.into_observation().ok_or("command exit != 0")?.bytes
                    }
                    Err(_) => return Err(format!("bad --source: {source}")),
                }
            };
            // Only identical *complete* content rebinds — a matching range
            // fragment is never enough.
            if new_bytes != recorded {
                return Err(
                    "replace refused: new source content differs from recorded full content".into(),
                );
            }
            // Rebind: which records share this version id (impact report).
            let mut affected = Vec::new();
            if let Ok(rd) = std::fs::read_dir(root.join("commits")) {
                for e in rd.flatten() {
                    if let Ok(txt) = std::fs::read_to_string(e.path())
                        && let Ok(c) = toml::from_str::<omd::records::commit::Commit>(&txt)
                        && c.content_ref == commit.content_ref
                    {
                        let cid = e.path().file_stem().unwrap().to_string_lossy().to_string();
                        affected.push(cid);
                    }
                }
            }
            // Persist an immutable binding revision pointing the version at
            // the new acquisition. Content/id/links/notes unchanged.
            let mut idb = [0u8; 16];
            omd::testing::Rng::fill(&OsRng, &mut idb);
            let binding = omd::records::binding::Binding {
                id: hex::encode(idb),
                version_id: commit.content_ref.clone(),
                acquisition: serde_json::json!({ "source": source }),
                seq: store.state().publication + 1,
                affected: affected.clone(),
            };
            omd::records::binding::write_binding(&root, &binding).map_err(|e| e.to_string())?;
            let mut st = store.state().clone();
            st.bindings
                .insert(commit.content_ref.clone(), binding.id.clone());
            st.publication += 1;
            // publish state via a marker-free path — bindings update is a
            // metadata revision, not a business commit.
            store.set_state(st).map_err(|e| e.to_string())?;
            Ok(
                serde_json::json!({ "ok": true, "binding": binding.id, "version": commit.content_ref, "affected": affected }),
            )
        }
        Cmd::Register { store_id, locator } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let mut st = store.state().clone();
            omd::records::cross::register_peer(&mut st, store_id, locator);
            store.set_state(st).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "registered": store_id }))
        }
        Cmd::Protect {
            target,
            peer,
            record,
        } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let mut st = store.state().clone();
            let cred = omd::records::cross::persist_inbound(&mut st, peer, record, target);
            store.set_state(st).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "credential": cred, "target": target }))
        }
        Cmd::Activate => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let mut st = store.state().clone();
            let id = omd::records::cross::activate(&mut st, &OsRng);
            store.set_state(st).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "store_id": id }))
        }
        Cmd::Gc { content } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            if !omd::records::cross::activated(store.state()) {
                return Err(
                    "store is an unregistered copy: gc refused until `omd activate`".into(),
                );
            }
            // Content gc: a local content copy is collectible only if no
            // version still references it AND no binding still points a
            // version at it. Conservative: unreadable peers + inbound
            // credentials retain their targets.
            let mut released = Vec::new();
            if *content {
                let mut protected = std::collections::HashSet::new();
                // Every version's sha256 is potentially needed.
                if let Ok(rd) = std::fs::read_dir(root.join("versions")) {
                    for e in rd.flatten() {
                        if let Ok(txt) = std::fs::read_to_string(e.path())
                            && let Ok(v) =
                                toml::from_str::<omd::records::version::SourceVersion>(&txt)
                        {
                            protected.insert(v.sha256.clone());
                        }
                    }
                }
                // Inbound credentials protect their targets' content — and
                // the TRANSITIVE closure: a protected target's own
                // previous_id / content / link basis must be retained so the
                // target remains recoverable, never just the direct object.
                for cred in store.state().inbound.values() {
                    protected.insert(cred.target.clone());
                    // Walk the protected commit's chain basis.
                    if let Ok(c) = store.read_commit(&cred.target) {
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
                if let Ok(rd) = std::fs::read_dir(root.join("content")) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().to_string();
                        if !protected.contains(&name) {
                            let _ = std::fs::remove_file(e.path());
                            released.push(name);
                        }
                    }
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
                for cred in store.state().inbound.values() {
                    keep.insert(cred.target.clone());
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
                // Candidates: commits on disk not reachable/kept.
                if let Ok(rd) = std::fs::read_dir(root.join("commits")) {
                    for e in rd.flatten() {
                        let id = e.path().file_stem().unwrap().to_string_lossy().to_string();
                        if !keep.contains(&id) {
                            let _ = std::fs::remove_file(e.path());
                            collected.push(id);
                        }
                    }
                }
            }
            // Report WHY each target is retained — the offline consumer's
            // peer_store_id + the protected target, never a bare count. The
            // credential is durable even when the consumer is unreachable.
            let protection_reasons: Vec<serde_json::Value> = store
                .state()
                .inbound
                .values()
                .map(|c| {
                    serde_json::json!({
                        "consumer": c.peer_store_id,
                        "target": c.target,
                        "reason": "inbound credential retained (consumer may be offline)",
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "ok": true, "released": released, "collected_commits": collected,
                "protected_count": store.state().inbound.len(),
                "protection_reasons": protection_reasons,
            }))
        }
        Cmd::Reindex => {
            // Rebuild the query index from the published manifest — a derived
            // cache, never a second authority. We regenerate `index.txt`
            // (a stand-in for the SQLite index: sorted commit-id → node map)
            // purely from the durable manifest + commit records. Never runs
            // a source program or reads Git objects to fill gaps.
            let manifest = std::fs::read_to_string(root.join("published")).unwrap_or_default();
            let mut rows = Vec::new();
            for id in manifest.lines().filter(|l| !l.is_empty()) {
                if let Ok(c) = Store::open(&root).and_then(|s| s.read_commit(id)) {
                    rows.push(format!(
                        "{}\t{}\t{}",
                        id,
                        omd::records::commit::kind_name(c.kind),
                        c.content_ref
                    ));
                }
            }
            rows.sort();
            std::fs::write(root.join("index.txt"), rows.join("\n")).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "indexed": rows.len() }))
        }
        Cmd::Log { id } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            // Walk the node's chain from its tip via previous_id.
            let chain = log_chain(&store, &root, id);
            Ok(serde_json::json!({ "ok": true, "node": id, "chain": chain }))
        }
        Cmd::Tree { id, level } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            // `--level file` caps at file level; a number caps depth.
            let (max_depth, file_only) = match level.as_deref() {
                Some("file") => (1usize, true),
                Some(n) => (n.parse().unwrap_or(32), false),
                None => (32usize, false),
            };
            let tree = mount_tree(&store, id.as_deref(), max_depth, file_only);
            Ok(tree)
        }
        Cmd::List { dangling } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            if *dangling {
                Ok(serde_json::json!({ "ok": true, "dangling": dangling_ids(&store, &root) }))
            } else {
                Ok(serde_json::json!({ "ok": true, "tips": store.state().tips }))
            }
        }
    }
}

/// Member file-nodes of `tag`: every file under a dir tagged `tag` (from the
/// live FS) plus any node tagged directly. Unmarked content stays in the
/// denominator — a tag without links is a coverage gap, not a silent pass.
fn tag_member_nodes(store: &Store, proj_root: &std::path::Path, tag: &str) -> Vec<String> {
    let mut members = std::collections::BTreeSet::new();
    // Directly-tagged nodes.
    for (node, tags) in &store.state().tags {
        if tags.contains(tag) && node.starts_with("file:") {
            members.insert(node.clone());
            // If it's a dir, walk its files (live FS → late members count).
            let dir = proj_root.join(node.strip_prefix("file:").unwrap());
            if dir.is_dir() {
                for f in omd::sources::scope::resolve(&dir, &[]).files {
                    members.insert(format!(
                        "file:{}/{}",
                        node.strip_prefix("file:").unwrap(),
                        f.display()
                    ));
                }
            }
        }
    }
    members.into_iter().collect()
}

/// Positions in `node`'s range children that carry a link to any node
/// tagged `target_tag`. Uses the persisted links' source ranges.
fn count_linked_positions(store: &Store, node: &str, target_tag: &str) -> u64 {
    let mut covered = std::collections::BTreeSet::new();
    for link in store.state().links.values() {
        // link.source is a range key under this file node
        if omd::relations::node::parent_of(&link.source) == *node {
            let target_tags = omd::relations::tags::resolve_tags(
                store.state(),
                &omd::relations::node::parent_of(&link.target),
            );
            if target_tags.contains(target_tag)
                && let Some((_, s, e)) = parse_span(&link.source)
            {
                for p in s..e {
                    covered.insert(p);
                }
            }
        }
    }
    covered.len() as u64
}

/// `range:path@mode:s-e` → (mode, s, e).
fn parse_span(key: &str) -> Option<(char, u64, u64)> {
    let at = key.find('@')?;
    let span = &key[at + 1..];
    // A nonce suffix (`0-5#<nonce>`) marks a parallel chain over identical
    // coords — the span is still `s-e`; strip the nonce before parsing.
    let span = span.split('#').next().unwrap_or(span);
    let (mode, rest) = span.split_once(':')?;
    let (s, e) = rest.split_once('-')?;
    Some((mode.chars().next()?, s.parse().ok()?, e.parse().ok()?))
}

/// Non-whitespace denominator for a file node's content (text mode).
fn file_denom(proj_root: &std::path::Path, node: &str) -> u64 {
    let path = node.strip_prefix("file:").unwrap_or(node);
    let full = proj_root.join(path);
    std::fs::read_to_string(&full)
        .map(|c| c.chars().filter(|ch| !ch.is_whitespace()).count() as u64)
        .unwrap_or(0)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(v) => {
            // `run` returns a payload; wrap in the contract envelope.
            let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(true);
            emit(&cli, envelope(ok, v, serde_json::json!([])));
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            // Classify the failure into the contract exit codes:
            // 1 check-fail/incomplete, 2 usage, 3 version conflict,
            // 4 lock conflict, 5 I/O/exec.
            let (code, kind) = if e.contains("lock") || e.contains("Lock") {
                (4, "lock_conflict")
            } else if e.contains("publication") || e.contains("mismatch") || e.contains("conflict")
            {
                (3, "version_conflict")
            } else if e.contains("bad --range") || e.contains("unknown") || e.contains("usage") {
                (2, "usage")
            } else if e.contains("io:") || e.contains("exec") {
                (5, "io_exec")
            } else {
                (1, "check_failed")
            };
            let diag = serde_json::json!([{ "kind": kind, "severity": "error",
                "message": e, "store": null, "node": null, "commit_id": null }]);
            let out = envelope(false, serde_json::json!(null), diag);
            if cli.json {
                println!("{}", serde_json::to_string(&out).unwrap());
            } else {
                eprintln!("error: {}", serde_json::to_string_pretty(&out).unwrap());
            }
            ExitCode::from(code)
        }
    }
}
