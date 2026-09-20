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

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// `commit init` — first record for a source.
    Init { path: String },
    /// `commit <kind>` — ordinary/marker/lifecycle commits.
    Commit {
        kind: String,
        path: String,
        #[arg(long)] reason: Option<String>,
        #[arg(long)] range: Option<String>,
        #[arg(long)] timestamp: Option<String>,
    },
    /// `omd verify` — full-store check (distinct from `commit verify <path>`).
    Verify { path: Option<String> },
    /// `omd check` — content/link coverage report.
    Check { path: Option<String> },
    /// `omd import` — alias for `commit import` (statistics inclusion).
    Import { path: String },
    /// `omd remove` — alias for `commit remove` (statistics removal).
    Remove { path: String },
    /// `omd delete` — alias for `commit delete` (tombstone).
    Delete { path: String },
    /// `omd rename` — alias for `commit rename` (source→target).
    Rename { source: String, target: String },
    /// `omd copy` — new initialization without prior association.
    Copy { source: String, target: String },
    /// `omd log <id>` — walk a node's commit chain.
    Log { id: String },
    /// `omd tree [id]` — mount-tree view from root or a node.
    Tree { id: Option<String> },
    /// `omd list` — query state (e.g. `--dangling`).
    List { #[arg(long)] dangling: bool },
}

/// Read a commit's previous_id from its on-disk record.
fn commit_prev(root: &Path, id: &str) -> Option<String> {
    let s = std::fs::read_to_string(root.join(format!("commits/{id}.toml"))).ok()?;
    let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
    Some(c.previous_id)
}

/// Walk a node chain tip→root via previous_id links (tip-first order).
fn log_chain(store: &Store, root: &Path, node: &str) -> Vec<String> {
    let tip = store.state().tips.get(node).cloned().unwrap_or_default();
    let mut out = Vec::new();
    let mut cur = tip;
    while !cur.is_empty() {
        out.push(cur.clone());
        cur = commit_prev(root, &cur).unwrap_or_default();
    }
    out
}

/// Render the mount tree as nested JSON from `start` (or the implicit root).
fn mount_tree(store: &Store, start: Option<&str>) -> serde_json::Value {
    let mounts = &store.state().mounts;
    fn build(node: &str, mounts: &std::collections::BTreeMap<String, Vec<String>>, depth: usize) -> serde_json::Value {
        if depth > 32 {
            return serde_json::json!({ "node": node, "truncated": true });
        }
        let children: Vec<serde_json::Value> = mounts
            .get(node)
            .map(|c| c.iter().map(|ch| build(ch, mounts, depth + 1)).collect())
            .unwrap_or_default();
        serde_json::json!({ "node": node, "children": children })
    }
    let root = start.unwrap_or("root");
    serde_json::json!({ "ok": true, "tree": build(root, mounts, 0) })
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
    cli.meta.clone().unwrap_or_else(|| PathBuf::from(".omd"))
}

fn emit(cli: &Cli, v: serde_json::Value) {
    if cli.json {
        println!("{}", serde_json::to_string(&v).unwrap());
    } else {
        // Human-readable fallback: still a JSON shape for now (single
        // canonical output; a pretty printer is a later task).
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    }
}

fn run(cli: &Cli) -> Result<serde_json::Value, String> {
    let root = meta_root(cli);
    match &cli.cmd {
        Cmd::Init { path } | Cmd::Import { path } | Cmd::Remove { path } | Cmd::Delete { path } => {
            let kind = match &cli.cmd {
                Cmd::Init { .. } => CommitKind::Init,
                Cmd::Import { .. } => CommitKind::Import,
                Cmd::Remove { .. } => CommitKind::Remove,
                Cmd::Delete { .. } => CommitKind::Delete,
                _ => unreachable!(),
            };
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let p = Path::new(path);
            let node = format!("file:{path}");
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            let cid = pipeline::commit_file(
                &mut store, &mut NoProbe, &OsRng, &SystemClock,
                &node, p, kind, payload, &Expected::default(),
            ).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") }))
        }
        Cmd::Commit { kind, path, reason, range, timestamp } => {
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
            let node = format!("file:{path}");
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            if let Some(r) = reason { payload.insert("reason".into(), r.clone().into()); }
            if let Some(r) = range { payload.insert("range".into(), r.clone().into()); }
            let _ = timestamp; // manual replay timestamp — wired in commit path later
            // State-only kinds observe no file; content kinds read the file.
            let state_only = matches!(kind,
                CommitKind::Unclean | CommitKind::Clean |
                CommitKind::AtomicBegin | CommitKind::AtomicEnd |
                CommitKind::Reset | CommitKind::ScopeAdjust | CommitKind::Tag);
            let cid = if state_only {
                pipeline::commit_marker(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, kind, payload, &Expected::default(),
                ).map_err(|e| e.to_string())?
            } else {
                pipeline::commit_file(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, Path::new(path), kind, payload, &Expected::default(),
                ).map_err(|e| e.to_string())?
            };
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") }))
        }
        Cmd::Verify { .. } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            let rep = pipeline::verify(&store);
            Ok(serde_json::to_value(rep).unwrap())
        }
        Cmd::Check { .. } => {
            Ok(serde_json::json!({ "ok": true, "note": "coverage check — wired in coverage tasks" }))
        }
        Cmd::Rename { .. } | Cmd::Copy { .. } => {
            Ok(serde_json::json!({ "ok": true, "note": "lifecycle verb — wired in path-lifecycle tasks" }))
        }
        Cmd::Log { id } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            // Walk the node's chain from its tip via previous_id.
            let chain = log_chain(&store, &root, id);
            Ok(serde_json::json!({ "ok": true, "node": id, "chain": chain }))
        }
        Cmd::Tree { id } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            let tree = mount_tree(&store, id.as_deref());
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(v) => {
            emit(&cli, v);
            ExitCode::SUCCESS
        }
        Err(e) => {
            if cli.json {
                println!("{}", serde_json::json!({ "ok": false, "error": e }));
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}
