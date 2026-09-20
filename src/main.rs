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
        /// `commit link --source A@.. --target B@..`
        #[arg(long)] source: Option<String>,
        #[arg(long)] target: Option<String>,
        /// `commit adapt --link-id L --changes c1,c2 --reason r`
        #[arg(long)] link_id: Option<String>,
        #[arg(long)] changes: Option<String>,
        /// `commit adapt --stop` — source-side branch stop after handling.
        #[arg(long)] stop: bool,
        /// `commit clean --no-reason` — explicitly omit the stop reason.
        #[arg(long = "no-reason")] no_reason: bool,
        /// `commit ... --link-from R` — create R→this link in the block.
        #[arg(long = "link-from")] link_from: Vec<String>,
        /// `commit ... --link-to R` — create this→R link in the block.
        #[arg(long = "link-to")] link_to: Vec<String>,
        /// `--id <commit>` — append to an existing range chain vs create a new
        /// one (same coords without --id = a new independent range object).
        #[arg(long)] id: Option<String>,
        /// `commit tag <path> --tag <name>` — flat project-local tag.
        #[arg(long)] tag: Option<String>,
        /// `commit scope_adjust --rule "spec->code" --level fail` — declare a
        /// named tag-link rule checked by `check`.
        #[arg(long)] rule: Option<String>,
        /// Rule severity: `warn` or `fail` (default `fail`).
        #[arg(long)] level: Option<String>,
        /// Explicitly skip the named rule — skip never confirms content.
        #[arg(long)] skip: bool,
    },
    /// `omd verify` — full-store check (distinct from `commit verify <path>`).
    Verify { path: Option<String> },
    /// `omd check` — content/link coverage report.
    Check { path: Option<String> },
    /// `omd import` — alias for `commit import` (statistics inclusion).
    /// Includes subdirs; `--exclude`/`--include` layer patterns over the scope.
    Import {
        path: String,
        #[arg(long = "exclude")] exclude: Vec<String>,
        #[arg(long = "include")] include: Vec<String>,
    },
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
        Cmd::Delete { path } => {
            let mut store = Store::open(&root).map_err(|e| e.to_string())?;
            let cid = pipeline::commit_lifecycle(
                &mut store, &mut NoProbe, &OsRng, &SystemClock,
                CommitKind::Delete, path, None, "", &Expected::default(),
            ).map_err(|e| e.to_string())?;
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
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            if let Cmd::Import { exclude, include, .. } = &cli.cmd {
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
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, kind, payload, &Expected::default(),
                ).map_err(|e| e.to_string())?
            } else {
                pipeline::commit_file(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, p, kind, payload, &Expected::default(),
                ).map_err(|e| e.to_string())?
            };
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") }))
        }
        Cmd::Commit { kind, path, reason, range, timestamp, source, target, link_id, changes, stop, no_reason, link_from, link_to, id, tag, rule, level, skip } => {
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
            // 8.4 pre-check: same-direction duplicate *resolved* range in
            // one command rejects the whole command before any write — never
            // silently dedup or create two identical links.
            {
                let mut seen_from = std::collections::HashSet::new();
                for r in link_from {
                    if !seen_from.insert(r) {
                        return Err(format!("duplicate --link-from range in one command: {r}"));
                    }
                }
                let mut seen_to = std::collections::HashSet::new();
                for r in link_to {
                    if !seen_to.insert(r) {
                        return Err(format!("duplicate --link-to range in one command: {r}"));
                    }
                }
            }
            // Resolve the node: --range targets a first-class range chain
            // mounted under the file; bare path targets the file chain.
            let node = match (range, id) {
                // --id names an existing chain node key to append to.
                (_, Some(node_id)) => node_id.clone(),
                // New independent range over identical coords gets a nonce.
                (Some(r), None) => {
                    let (mode, s, e) = omd::relations::node::parse_range_arg(r)
                        .ok_or_else(|| format!("bad --range: {r}"))?;
                    let base = omd::relations::node::range_key(path, mode, s, e);
                    if store.state().tips.contains_key(&base) {
                        // Same coords, no --id: a NEW independent object.
                        let nonce = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
                        omd::relations::node::range_key_nonce(path, mode, s, e, &nonce)
                    } else {
                        base
                    }
                }
                (None, None) => format!("file:{path}"),
            };
            let mut payload = serde_json::Map::new();
            payload.insert("path".into(), path.clone().into());
            if let Some(r) = reason { payload.insert("reason".into(), r.clone().into()); }
            if let Some(r) = range { payload.insert("range".into(), r.clone().into()); }
            if *no_reason { payload.insert("no_reason".into(), true.into()); }
            if let Some(t) = tag { payload.insert("tag".into(), t.clone().into()); }
            if let Some(r) = rule { payload.insert("rule".into(), r.clone().into()); }
            if let Some(l) = level { payload.insert("level".into(), l.clone().into()); }
            if *skip { payload.insert("skip".into(), true.into()); }
            let _ = timestamp; // manual replay timestamp — wired in commit path later
            // State-only kinds observe no file; content kinds read the file.
            let state_only = matches!(kind,
                CommitKind::Unclean | CommitKind::Clean |
                CommitKind::AtomicBegin | CommitKind::AtomicEnd |
                CommitKind::Reset | CommitKind::ScopeAdjust | CommitKind::Tag);
            // --link-from/--link-to wrap the commit in an ATOMIC block: the
            // range commit + each link creation are recorded inside one
            // BEGIN/END on this node's chain (spec 8.4 atomic combination).
            let combo = !link_from.is_empty() || !link_to.is_empty();
            if combo {
                let mut pl = serde_json::Map::new();
                pl.insert("path".into(), path.clone().into());
                pipeline::commit_marker(&mut store, &mut NoProbe, &OsRng, &SystemClock, &node, CommitKind::AtomicBegin, pl, &Expected::default()).map_err(|e| e.to_string())?;
            }
            let cid = if kind == CommitKind::Link {
                let src = source.clone().unwrap_or_default();
                let tgt = target.clone().unwrap_or_default();
                let r = reason.clone().unwrap_or_default();
                pipeline::commit_link(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, &src, &tgt, &r, &Expected::default(),
                ).map_err(|e| e.to_string())?
            } else if kind == CommitKind::Adapt {
                let lid = link_id.clone().unwrap_or_default();
                let ch: Vec<String> = changes.clone().unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let r = reason.clone().unwrap_or_default();
                pipeline::commit_adapt(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, &lid, &ch, &r, *stop, &Expected::default(),
                ).map_err(|e| e.to_string())?
            } else if kind == CommitKind::Reset {
                // reset resolves the target's kind+prev from its on-disk record.
                let target = range.clone().or(reason.clone()).unwrap_or_default();
                let mut lk = |id: &str| -> Option<(CommitKind, String)> {
                    let s = std::fs::read_to_string(root.join(format!("commits/{id}.toml"))).ok()?;
                    let c: omd::records::commit::Commit = toml::from_str(&s).ok()?;
                    Some((c.kind, c.previous_id))
                };
                match pipeline::reset(&node, &target, &mut lk) {
                    Ok(out) => {
                        let mut pl = serde_json::Map::new();
                        pl.insert("requested".into(), out.requested.clone().into());
                        pl.insert("actual".into(), out.actual.clone().into());
                        pl.insert("warning".into(), out.warning.clone().into());
                        // Record the reset as a commit on this node's chain.
                        pipeline::commit_marker(
                            &mut store, &mut NoProbe, &OsRng, &SystemClock,
                            &node, CommitKind::Reset, pl, &Expected::default(),
                        ).map_err(|e| e.to_string())?
                    }
                    Err(e) => return Err(e.to_string()),
                }
            } else if state_only {
                pipeline::commit_marker(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, kind, payload, &Expected::default(),
                ).map_err(|e| e.to_string())?
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
                }
                pipeline::commit_file(
                    &mut store, &mut NoProbe, &OsRng, &SystemClock,
                    &node, Path::new(path), kind, payload, &Expected::default(),
                ).map_err(|e| e.to_string())?
            };
            // Create the requested links inside the block, then close it.
            if combo {
                for r in link_from {
                    pipeline::commit_link(&mut store, &mut NoProbe, &OsRng, &SystemClock, &node, r, &node, "combo" , &Expected::default()).map_err(|e| e.to_string())?;
                }
                for r in link_to {
                    pipeline::commit_link(&mut store, &mut NoProbe, &OsRng, &SystemClock, &node, &node, r, "combo", &Expected::default()).map_err(|e| e.to_string())?;
                }
                let mut pl = serde_json::Map::new();
                pl.insert("path".into(), path.clone().into());
                pipeline::commit_marker(&mut store, &mut NoProbe, &OsRng, &SystemClock, &node, CommitKind::AtomicEnd, pl, &Expected::default()).map_err(|e| e.to_string())?;
            }
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": format!("{kind:?}") }))
        }
        Cmd::Verify { .. } => {
            let store = Store::open(&root).map_err(|e| e.to_string())?;
            let rep = pipeline::verify(&store);
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
                    if let Ok(txt) = std::fs::read_to_string(e.path()) {
                        if let Ok(c) = toml::from_str::<omd::records::commit::Commit>(&txt) {
                            if c.kind == omd::records::commit::CommitKind::Import {
                                let path = c.payload.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let scope: Vec<String> = c.payload.get("scope")
                                    .and_then(|v| v.as_array())
                                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                                    .unwrap_or_default();
                                scopes.push((path, scope));
                            }
                        }
                    }
                }
            }
            let mut report = serde_json::Map::new();
            let mut all_files = Vec::new();
            let proj_root = std::env::current_dir().unwrap_or_default();
            for (dir, pats) in &scopes {
                let base = if dir.is_empty() { proj_root.clone() } else { proj_root.join(dir) };
                let scope = omd::sources::scope::resolve(&base, pats);
                for f in &scope.files {
                    let node = format!("file:{}/{}", dir, f.display());
                    let covered = store.state().tips.contains_key(&node);
                    all_files.push(serde_json::json!({
                        "file": f.display().to_string(),
                        "scope": dir,
                        "tracked": covered,
                    }));
                }
                if !scope.problems.is_empty() {
                    report.insert("problems".into(), scope.problems.clone().into());
                }
            }
            report.insert("files".into(), all_files.into());

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
                let forward_ok = fd == 0 || fc >= fd;
                let mut pass = forward_ok;
                if two_way {
                    let (rc, rd) = cov(tgt_tag, src_tag);
                    pass = pass && (rd == 0 || rc >= rd);
                }
                let status = if rule.skip { "skipped" } else if pass { "pass" } else { "fail" };
                if !rule.skip && !pass && rule.level == "fail" { check_failed = true; }
                rules_out.push(serde_json::json!({
                    "rule": name, "level": rule.level, "status": status,
                    "coverage": if fd == 0 { serde_json::Value::Null } else { serde_json::json!(fc * 100 / fd) },
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
                &mut store, &mut NoProbe, &OsRng, &SystemClock,
                CommitKind::Rename, source, Some(target), "", &Expected::default(),
            ).map_err(|e| e.to_string())?;
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
                &mut store, &mut NoProbe, &OsRng, &SystemClock,
                &node, Path::new(target), CommitKind::Init, payload, &Expected::default(),
            ).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true, "commit": cid, "kind": "Init", "copied_from": source }))
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

/// Member file-nodes of `tag`: every file under a dir tagged `tag` (from the
/// live FS) plus any node tagged directly. Unmarked content stays in the
/// denominator — a tag without links is a coverage gap, not a silent pass.
fn tag_member_nodes(store: &Store, proj_root: &std::path::Path, tag: &str) -> Vec<String> {
    let mut members = std::collections::BTreeSet::new();
    // Directly-tagged nodes.
    for (node, tags) in &store.state().tags {
        if tags.contains(tag) {
            if node.starts_with("file:") {
                members.insert(node.clone());
                // If it's a dir, walk its files (live FS → late members count).
                let dir = proj_root.join(node.strip_prefix("file:").unwrap());
                if dir.is_dir() {
                    for f in omd::sources::scope::resolve(&dir, &[]).files {
                        members.insert(format!("file:{}/{}",
                            node.strip_prefix("file:").unwrap(), f.display()));
                    }
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
    for (_, link) in &store.state().links {
        // link.source is a range key under this file node
        if omd::relations::node::parent_of(&link.source) == *node {
            let target_tags = omd::relations::tags::resolve_tags(store.state(), &omd::relations::node::parent_of(&link.target));
            if target_tags.contains(target_tag) {
                if let Some((_, s, e)) = parse_span(&link.source) {
                    for p in s..e { covered.insert(p); }
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
    let (mode, rest) = span.split_once(':')?;
    let (s, e) = rest.split('-').next().map(|_|()).and_then(|_| rest.split_once('-'))?;
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
