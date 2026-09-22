//! S3 cross-store protocol: real stores, caller evidence, ordered locks,
//! immutable peer/inbound revisions, copy activation, and conservative GC.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard};

use omd::records::commit::CommitKind;
use omd::records::pipeline;
use omd::records::store::{Expected, NoProbe, Store};
use omd::records::time::{OsRng, SystemClock};
use omd::testing::{FaultInjector, PublishStage};

fn omd() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_omd") {
        return path.into();
    }
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("omd");
    path
}

static LIBRARY_CONFIG_LOCK: Mutex<()> = Mutex::new(());

struct LibraryConfigGuard {
    _lock: MutexGuard<'static, ()>,
    previous: Option<OsString>,
}

impl Drop for LibraryConfigGuard {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("OMD_CONFIG_PATH", value),
                None => std::env::remove_var("OMD_CONFIG_PATH"),
            }
        }
    }
}

struct Env {
    root: tempfile::TempDir,
    home: PathBuf,
    config: PathBuf,
    cache: PathBuf,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        Self {
            home: root.path().join("home"),
            config: root.path().join("config"),
            cache: root.path().join("cache"),
            root,
        }
    }

    fn library_config(&self) -> LibraryConfigGuard {
        let lock = LIBRARY_CONFIG_LOCK.lock().unwrap();
        let previous = std::env::var_os("OMD_CONFIG_PATH");
        unsafe { std::env::set_var("OMD_CONFIG_PATH", &self.config) };
        LibraryConfigGuard {
            _lock: lock,
            previous,
        }
    }

    fn store(&self, name: &str) -> TestStore {
        let root = self.root.path().join(name);
        std::fs::create_dir_all(&root).unwrap();
        TestStore {
            root: root.clone(),
            meta: root.join(".omd"),
        }
    }

    fn command(&self, store: &TestStore, args: &[&str], auto_expected: bool) -> Output {
        let args = if auto_expected {
            common::with_expected(
                &omd(),
                &store.root,
                args,
                Some(&store.meta),
                Some(&self.home),
                Some(&self.config),
                Some(&self.cache),
            )
        } else {
            args.iter().map(|arg| (*arg).to_string()).collect()
        };
        Command::new(omd())
            .arg("--json")
            .arg("--root")
            .arg(&store.root)
            .arg("--meta")
            .arg(&store.meta)
            .args(args)
            .env("HOME", &self.home)
            .env("OMD_CONFIG_PATH", &self.config)
            .env("OMD_CACHE_PATH", &self.cache)
            .current_dir(&store.root)
            .output()
            .unwrap()
    }

    fn run(&self, store: &TestStore, args: &[&str]) -> serde_json::Value {
        let output = self.command(store, args, true);
        assert_ok(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn expected(&self, store: &TestStore) -> Expected {
        let output = self.command(store, &["verify"], false);
        assert_ok(&output);
        serde_json::from_value(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["data"]
                ["expected"]
                .clone(),
        )
        .unwrap()
    }

    fn register_alias(&self, store: &TestStore, alias: &str) {
        let expected = serde_json::to_string(&self.expected(store)).unwrap();
        let output = self.command(
            store,
            &[
                "--expected",
                &expected,
                "project",
                "register",
                alias,
                store.root.to_str().unwrap(),
                store.meta.to_str().unwrap(),
            ],
            false,
        );
        assert_ok(&output);
    }

    fn register_peer(&self, owner: &TestStore, peer: &TestStore) {
        let owner_expected = serde_json::to_string(&self.expected(owner)).unwrap();
        let peer_expected = serde_json::to_string(&self.expected(peer)).unwrap();
        let peer_id = peer.info().store_id;
        let output = self.command(
            owner,
            &[
                "--expected",
                &owner_expected,
                "register",
                &peer_id,
                peer.root.to_str().unwrap(),
                peer.meta.to_str().unwrap(),
                "--peer-expected",
                &peer_expected,
            ],
            false,
        );
        assert_ok(&output);
    }
}

#[derive(Clone)]
struct TestStore {
    root: PathBuf,
    meta: PathBuf,
}

#[derive(Debug, Clone)]
struct StoreInfo {
    project_id: String,
    store_id: String,
    ranges: BTreeMap<String, String>,
    links: BTreeMap<String, toml::Value>,
    inbound: BTreeMap<String, String>,
}

impl TestStore {
    fn write(&self, path: &str, content: &str) {
        if let Some(parent) = self.root.join(path).parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(self.root.join(path), content).unwrap();
    }

    fn info(&self) -> StoreInfo {
        let state: toml::Value =
            toml::from_str(&std::fs::read_to_string(self.meta.join("state.toml")).unwrap())
                .unwrap();
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string(self.meta.join("manifest.toml")).unwrap())
                .unwrap();
        let ranges = state["tips"]
            .as_table()
            .unwrap()
            .iter()
            .filter(|(key, _)| key.starts_with("range:"))
            .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_string()))
            .collect();
        let links = state
            .get("links")
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let inbound = state
            .get("inbound")
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| (key, value.as_str().unwrap().to_string()))
            .collect();
        StoreInfo {
            project_id: manifest["project_id"].as_str().unwrap().to_string(),
            store_id: state["store_id"].as_str().unwrap().to_string(),
            ranges,
            links,
            inbound,
        }
    }

    fn bytes(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.file_name().is_some_and(|name| name == "write.lock") {
                    continue;
                }
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        std::fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.meta, &self.meta, &mut out);
        out
    }
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("not json: {}", String::from_utf8_lossy(&output.stdout)))
}

fn init_range(env: &Env, store: &TestStore, name: &str, body: &str) -> (String, String) {
    let before: BTreeSet<String> = if store.meta.exists() {
        store.info().ranges.into_keys().collect()
    } else {
        BTreeSet::new()
    };
    store.write(name, body);
    env.run(store, &["init", name]);
    env.run(
        store,
        &[
            "commit", "commit", name, "--range", "0", "4", "--reason", "track",
        ],
    );
    store
        .info()
        .ranges
        .into_iter()
        .find(|(node, _)| !before.contains(node))
        .unwrap()
}

fn link_to_peer(
    env: &Env,
    consumer: &TestStore,
    source_file: &str,
    source_tip: &str,
    target: &TestStore,
    target_tip: &str,
) -> String {
    let target_id = target.info().store_id;
    let value = env.run(
        consumer,
        &[
            "commit",
            "commit",
            source_file,
            "--id",
            source_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "cross",
            "--link-to-store",
            &target_id,
            target_tip,
        ],
    );
    value["data"]["links"][0].as_str().unwrap().to_string()
}

fn link_from_peer(
    env: &Env,
    consumer: &TestStore,
    target_file: &str,
    target_tip: &str,
    source: &TestStore,
    source_version: &str,
) -> String {
    let source_id = source.info().store_id;
    let value = env.run(
        consumer,
        &[
            "commit",
            "commit",
            target_file,
            "--id",
            target_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "incoming",
            "--link-from-store",
            &source_id,
            source_version,
        ],
    );
    value["data"]["links"][0].as_str().unwrap().to_string()
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        let dst = target.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(src, dst).unwrap();
        }
    }
}

#[test]
fn incoming_only_copy_activation_protects_exact_historical_source() {
    let alice = Env::new();
    let p = alice.store("peer-p");
    let s = alice.store("source-s");
    let (p_root, p0) = init_range(&alice, &p, "peer.md", "abcdefghij");
    let p1 = alice.run(
        &p,
        &[
            "commit",
            "commit",
            "peer.md",
            "--id",
            &p0,
            "--range",
            "0",
            "5",
            "--reason",
            "selected historical",
        ],
    )["data"]["commit"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, s_tip) = init_range(&alice, &s, "source.md", "klmnopqrst");
    alice.register_peer(&s, &p);
    alice.register_peer(&p, &s);
    let link_id = link_from_peer(&alice, &s, "source.md", &s_tip, &p, &p1);
    let link = s.info().links[&link_id].clone();
    assert!(
        link["source"]
            .as_str()
            .unwrap()
            .contains(&p.info().store_id)
    );
    assert_eq!(link["source_version"].as_str(), Some(p1.as_str()));

    alice.run(&p, &["commit", "reset", "peer.md", "--reset-target", &p1]);
    assert_ne!(
        p.info().ranges[&p_root],
        p1,
        "p1 is selected but no longer current"
    );

    let s_before = s.bytes();
    let copied_info = s.info();
    let copied_commits: BTreeSet<_> = std::fs::read_dir(s.meta.join("commits"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    let bob = Env::new();
    let t = bob.store("copy-t");
    copy_tree(&s.root, &t.root);
    let t_expected = serde_json::to_string(&bob.expected(&t)).unwrap();
    assert_ok(&bob.command(
        &t,
        &[
            "--expected",
            &t_expected,
            "project",
            "register",
            "bob-copy",
            t.root.to_str().unwrap(),
            t.meta.to_str().unwrap(),
        ],
        false,
    ));
    bob.register_peer(&t, &p);

    let activated = bob.run(&t, &["activate"]);
    let t_id = activated["data"]["store_id"].as_str().unwrap().to_string();
    let protections = activated["data"]["protections"].as_array().unwrap();
    assert_eq!(
        protections.len(),
        1,
        "incoming endpoint requires one protection"
    );
    let credential = protections[0].as_str().unwrap().to_string();
    let receipt: toml::Value = toml::from_str(
        &std::fs::read_to_string(p.meta.join(format!("inbound/{credential}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["consumer_store_id"].as_str(), Some(t_id.as_str()));
    assert_eq!(
        receipt["record_id"].as_str(),
        link["created_by"].as_str(),
        "activation protects exact copied creation commit"
    );
    assert_eq!(receipt["link_id"].as_str(), Some(link_id.as_str()));
    assert_eq!(receipt["target_root"].as_str(), Some(p_root.as_str()));
    assert_eq!(receipt["target_version"].as_str(), Some(p1.as_str()));

    let t_info = t.info();
    assert_eq!(t_info.project_id, copied_info.project_id);
    assert_eq!(
        t_info.ranges, copied_info.ranges,
        "copied commit tips unchanged"
    );
    assert_eq!(t_info.links[&link_id], link, "copied link unchanged");
    assert!(
        t.meta
            .join(format!(
                "commits/{}.toml",
                link["created_by"].as_str().unwrap()
            ))
            .is_file()
    );
    let activated_commits: BTreeSet<_> = std::fs::read_dir(t.meta.join("commits"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(activated_commits, copied_commits);
    assert_eq!(s.bytes(), s_before, "original S remains byte-identical");

    let p_expected = serde_json::to_string(&alice.expected(&p)).unwrap();
    let t_expected = serde_json::to_string(&bob.expected(&t)).unwrap();
    assert_ok(&alice.command(
        &p,
        &[
            "--expected",
            &p_expected,
            "register",
            &t_id,
            t.root.to_str().unwrap(),
            t.meta.to_str().unwrap(),
            "--peer-expected",
            &t_expected,
        ],
        false,
    ));
    let gc = alice.run(&p, &["gc", "--content"]);
    assert!(
        !gc["data"]["collected_protections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(credential.as_str()))
    );
    assert!(p.info().inbound.contains_key(&credential));
    let p_store = Store::open_existing(&p.meta).unwrap();
    let selected = p_store.read_commit(&p1).unwrap();
    let version = p_store.read_version(&selected.content_ref).unwrap();
    assert!(p.meta.join(format!("commits/{p1}.toml")).is_file());
    assert!(p.meta.join(format!("content/{}", version.sha256)).is_file());
}

#[test]
fn incoming_only_copy_activation_fails_closed_without_required_peer() {
    let alice = Env::new();
    let p = alice.store("peer-p");
    let s = alice.store("source-s");
    let (_, p_tip) = init_range(&alice, &p, "peer.md", "abcdefghij");
    let (_, s_tip) = init_range(&alice, &s, "source.md", "klmnopqrst");
    alice.register_peer(&s, &p);
    alice.register_peer(&p, &s);
    link_from_peer(&alice, &s, "source.md", &s_tip, &p, &p_tip);

    let bob = Env::new();
    let t = bob.store("copy-t");
    copy_tree(&s.root, &t.root);
    let t_expected = serde_json::to_string(&bob.expected(&t)).unwrap();
    assert_ok(&bob.command(
        &t,
        &[
            "--expected",
            &t_expected,
            "project",
            "register",
            "bob-copy",
            t.root.to_str().unwrap(),
            t.meta.to_str().unwrap(),
        ],
        false,
    ));
    let before_unobserved = t.bytes();
    let config_before_unobserved = std::fs::read(bob.config.join("projects.toml")).unwrap();
    let unobserved = bob.command(&t, &["activate"], true);
    assert!(!unobserved.status.success());
    assert_eq!(t.bytes(), before_unobserved);
    assert_eq!(
        std::fs::read(bob.config.join("projects.toml")).unwrap(),
        config_before_unobserved,
        "failed activation grants no configuration authority"
    );
    assert!(!omd::records::cross::activated(
        &Store::open_existing(&t.meta).unwrap()
    ));

    bob.register_peer(&t, &p);
    let before_offline = t.bytes();
    let config_before_offline = std::fs::read(bob.config.join("projects.toml")).unwrap();
    let inbound_before = p.info().inbound;
    let hidden = p.root.with_extension("offline");
    std::fs::rename(&p.root, &hidden).unwrap();
    let offline = bob.command(&t, &["activate"], true);
    assert!(!offline.status.success());
    assert_eq!(t.bytes(), before_offline);
    assert_eq!(
        std::fs::read(bob.config.join("projects.toml")).unwrap(),
        config_before_offline,
        "offline required peer grants no configuration authority"
    );
    std::fs::rename(&hidden, &p.root).unwrap();
    assert_eq!(
        p.info().inbound,
        inbound_before,
        "no activation protection published"
    );
    assert!(!omd::records::cross::activated(
        &Store::open_existing(&t.meta).unwrap()
    ));
}

#[test]
fn alice_bob_copy_activation_continues_t_without_changing_s_or_external_refs() {
    let alice = Env::new();
    let s = alice.store("alice-s");
    let b = alice.store("peer-b");
    let c = alice.store("consumer-c");
    let (_, s_tip) = init_range(&alice, &s, "doc.md", "abcdefghij");
    let (_, b_tip) = init_range(&alice, &b, "peer.md", "klmnopqrst");
    let (_, c_tip) = init_range(&alice, &c, "consumer.md", "uvwxyz0123");

    alice.register_peer(&s, &b);
    alice.register_peer(&b, &s);
    let l = link_to_peer(&alice, &s, "doc.md", &s_tip, &b, &b_tip);

    alice.register_peer(&c, &s);
    alice.register_peer(&s, &c);
    let incoming = link_to_peer(&alice, &c, "consumer.md", &c_tip, &s, &s_tip);
    let c_target_before = c.info().links[&incoming]["target"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(c_target_before.contains(&s.info().store_id));

    let s_before = s.bytes();
    let copied_info = s.info();
    assert!(copied_info.links.contains_key(&l));

    let bob = Env::new();
    let t = bob.store("bob-t");
    copy_tree(&s.root, &t.root);

    let write = bob.command(
        &t,
        &[
            "commit", "commit", "doc.md", "--id", &s_tip, "--range", "0", "4", "--reason", "no",
        ],
        true,
    );
    assert!(!write.status.success(), "raw copy must be read-only");
    assert_eq!(json(&write)["diagnostics"][0]["kind"], "check_failed");
    assert!(!bob.command(&t, &["gc"], true).status.success());

    let t_expected = serde_json::to_string(&bob.expected(&t)).unwrap();
    let mapped = bob.command(
        &t,
        &[
            "--expected",
            &t_expected,
            "project",
            "register",
            "bob-copy",
            t.root.to_str().unwrap(),
            t.meta.to_str().unwrap(),
        ],
        false,
    );
    assert_ok(&mapped);
    assert!(
        !bob.command(
            &t,
            &[
                "commit", "commit", "doc.md", "--id", &s_tip, "--range", "0", "4", "--reason",
                "still-no",
            ],
            true,
        )
        .status
        .success()
    );

    bob.register_peer(&t, &b);
    let activated = bob.run(&t, &["activate"]);
    let t_id = activated["data"]["store_id"].as_str().unwrap().to_string();
    let activation_receipts: Vec<String> = activated["data"]["protections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    assert_ne!(t_id, copied_info.store_id);
    assert_eq!(t.info().project_id, copied_info.project_id);
    assert!(t.info().links.contains_key(&l));

    let b_expected = serde_json::to_string(&alice.expected(&b)).unwrap();
    let t_expected = serde_json::to_string(&bob.expected(&t)).unwrap();
    let mapped_t = alice.command(
        &b,
        &[
            "--expected",
            &b_expected,
            "register",
            &t_id,
            t.root.to_str().unwrap(),
            t.meta.to_str().unwrap(),
            "--peer-expected",
            &t_expected,
        ],
        false,
    );
    assert_ok(&mapped_t);
    let gc = alice.run(&b, &["gc", "--content"]);
    assert!(
        gc["data"]["collected_protections"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let retained = b.info().inbound;
    assert!(
        activation_receipts
            .iter()
            .all(|credential| retained.contains_key(credential))
    );
    let live_link = t.info().links[&l].clone();
    for credential in &activation_receipts {
        let record: toml::Value = toml::from_str(
            &std::fs::read_to_string(b.meta.join(format!("inbound/{credential}.toml"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            record["record_id"].as_str(),
            live_link["created_by"].as_str()
        );
        assert_eq!(record["link_id"].as_str(), Some(l.as_str()));
    }

    t.write("doc.md", "abcdefghij changed");
    let t_tip_before = t.info().ranges.values().next().unwrap().clone();
    bob.run(
        &t,
        &[
            "commit",
            "commit",
            "doc.md",
            "--id",
            &t_tip_before,
            "--range",
            "0",
            "4",
            "--reason",
            "bob",
        ],
    );
    assert_ne!(t.info().ranges.values().next().unwrap(), &t_tip_before);
    assert!(t.info().links.contains_key(&l));
    assert_eq!(s.bytes(), s_before, "Alice S must remain byte-identical");
    assert_eq!(
        c.info().links[&incoming]["target"].as_str().unwrap(),
        c_target_before,
        "external S reference must not redirect to T"
    );
}

#[test]
fn activation_rejects_missing_or_stale_required_peer_before_local_publication() {
    let alice = Env::new();
    let s = alice.store("s");
    let b = alice.store("b");
    let (_, s_tip) = init_range(&alice, &s, "s.md", "abcdefghij");
    let (_, b_tip) = init_range(&alice, &b, "b.md", "klmnopqrst");
    alice.register_peer(&s, &b);
    alice.register_peer(&b, &s);
    link_to_peer(&alice, &s, "s.md", &s_tip, &b, &b_tip);

    let bob = Env::new();
    let t = bob.store("t");
    copy_tree(&s.root, &t.root);
    bob.register_peer(&t, &b);
    let before = t.bytes();

    let wrong = bob.store("wrong-peer");
    init_range(&bob, &wrong, "wrong.md", "wrongpeer0");
    let config_path = bob.config.join("projects.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let wrong_config = config.replace(b.meta.to_str().unwrap(), wrong.meta.to_str().unwrap());
    std::fs::write(&config_path, wrong_config).unwrap();
    let wrong_identity = bob.command(&t, &["activate"], true);
    assert!(!wrong_identity.status.success());
    assert_eq!(
        t.bytes(),
        before,
        "wrong mapped peer identity must be zero-write"
    );
    std::fs::write(&config_path, config).unwrap();

    let hidden = b.root.with_extension("offline");
    std::fs::rename(&b.root, &hidden).unwrap();
    let unavailable = bob.command(&t, &["activate"], true);
    assert!(!unavailable.status.success());
    assert_eq!(
        t.bytes(),
        before,
        "offline peer must reject before T publication"
    );
    std::fs::rename(&hidden, &b.root).unwrap();

    let stale = serde_json::to_string(&bob.expected(&t)).unwrap();
    b.write("b.md", "klmnopqrst changed");
    let b_current = b.info().ranges.values().next().unwrap().clone();
    alice.run(
        &b,
        &[
            "commit", "commit", "b.md", "--id", &b_current, "--range", "0", "4", "--reason",
            "advance",
        ],
    );
    let stale_attempt = bob.command(&t, &["--expected", &stale, "activate"], false);
    assert_eq!(stale_attempt.status.code(), Some(3));
    assert_eq!(
        t.bytes(),
        before,
        "stale evidence must reject before T publication"
    );
}

#[test]
fn protected_library_rejects_copied_peer_and_stale_mapping_before_writes() {
    let env = Env::new();
    let _library_config = env.library_config();
    let a = env.store("api-a");
    let b = env.store("api-b");
    let (a_root, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (b_root, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);

    let copied = env.store("api-b-copy");
    copy_tree(&b.root, &copied.root);
    let expected = env.expected(&a);
    let peer_expected = expected.peers.get(&b.info().store_id).unwrap().clone();
    let before_a = a.bytes();
    let before_b = b.bytes();
    let before_copy = copied.bytes();
    let mut a_store = Store::open_existing(&a.meta).unwrap();
    a_store.bind_context(
        expected.instance.clone(),
        expected.mapping_revision,
        a.root.clone(),
        None,
        a.root.clone(),
        Default::default(),
    );
    let mut copied_store = Store::open_existing(&copied.meta).unwrap();
    let copied_failure = pipeline::commit_xlink_protected(
        &mut a_store,
        &mut copied_store,
        &mut NoProbe,
        &OsRng,
        &SystemClock,
        &a_root,
        &a_root,
        &a_tip,
        &b.info().store_id,
        &b_root,
        &b_tip,
        "copied-peer-001",
        "copied peer",
        &expected,
        &peer_expected,
    )
    .unwrap_err();
    assert!(copied_failure.credential_id.is_none());
    drop(a_store);
    drop(copied_store);
    assert_eq!(a.bytes(), before_a);
    assert_eq!(b.bytes(), before_b);
    assert_eq!(copied.bytes(), before_copy);

    env.register_peer(&a, &b); // same logical peer, new machine-local mapping revision
    let before_a = a.bytes();
    let before_b = b.bytes();
    let mut a_store = Store::open_existing(&a.meta).unwrap();
    a_store.bind_context(
        expected.instance.clone(),
        expected.mapping_revision,
        a.root.clone(),
        None,
        a.root.clone(),
        Default::default(),
    );
    let mut b_store = Store::open_existing(&b.meta).unwrap();
    let stale_failure = pipeline::commit_xlink_protected(
        &mut a_store,
        &mut b_store,
        &mut NoProbe,
        &OsRng,
        &SystemClock,
        &a_root,
        &a_root,
        &a_tip,
        &b.info().store_id,
        &b_root,
        &b_tip,
        "stale-map-001",
        "stale mapping",
        &expected,
        &peer_expected,
    )
    .unwrap_err();
    assert!(stale_failure.credential_id.is_none());
    drop(a_store);
    drop(b_store);
    assert_eq!(a.bytes(), before_a);
    assert_eq!(b.bytes(), before_b);

    let fresh = env.expected(&a);
    let fresh_peer = fresh.peers.get(&b.info().store_id).unwrap().clone();
    let mut a_store = Store::open_existing(&a.meta).unwrap();
    a_store.bind_context(
        fresh.instance.clone(),
        fresh.mapping_revision,
        a.root.clone(),
        None,
        a.root.clone(),
        Default::default(),
    );
    let mut b_store = Store::open_existing(&b.meta).unwrap();
    let published = pipeline::commit_xlink_protected(
        &mut a_store,
        &mut b_store,
        &mut NoProbe,
        &OsRng,
        &SystemClock,
        &a_root,
        &a_root,
        &a_tip,
        &b.info().store_id,
        &b_root,
        &b_tip,
        "valid-peer-001",
        "valid peer",
        &fresh,
        &fresh_peer,
    )
    .unwrap();
    drop(a_store);
    drop(b_store);
    let a_info = a.info();
    assert!(a_info.links.contains_key(&published.link_id));
    assert!(b.info().inbound.contains_key(&published.credential_id));
    let b_store = Store::open_existing(&b.meta).unwrap();
    let receipt = omd::records::cross::read_inbound(&b_store, &published.credential_id).unwrap();
    let created_by = a_info.links[&published.link_id]["created_by"]
        .as_str()
        .unwrap();
    assert_eq!(receipt.record_id, created_by);
    assert_eq!(receipt.link_id, published.link_id);
    assert!(a.meta.join(format!("commits/{created_by}.toml")).is_file());

    // GC must not equate links-table absence with a never-published record:
    // the exact immutable record is still retained and protects its target.
    let state_path = a.meta.join("state.toml");
    let mut state: toml::Value =
        toml::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    state["links"]
        .as_table_mut()
        .unwrap()
        .remove(&published.link_id);
    std::fs::write(&state_path, toml::to_string_pretty(&state).unwrap()).unwrap();
    let gc = env.run(&b, &["gc"]);
    assert!(
        !gc["data"]["collected_protections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(published.credential_id.as_str()))
    );
    assert!(
        gc["data"]["protection_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value["reason"] == "exact consumer record remains retained")
    );
}

#[test]
fn raw_copy_init_delete_and_library_publish_are_zero_write() {
    let alice = Env::new();
    let source = alice.store("source");
    init_range(&alice, &source, "doc.md", "abcdefghij");
    let bob = Env::new();

    for action in ["init", "delete"] {
        let copy = bob.store(&format!("copy-{action}"));
        copy_tree(&source.root, &copy.root);
        if action == "init" {
            copy.write("new.md", "new content");
        }
        let observed = bob.expected(&copy);
        let before = copy.bytes();
        let expected_json = serde_json::to_string(&observed).unwrap();
        let path = if action == "init" { "new.md" } else { "doc.md" };
        let output = bob.command(&copy, &["--expected", &expected_json, action, path], false);
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(copy.bytes(), before);
    }

    let library_copy = bob.store("copy-library");
    copy_tree(&source.root, &library_copy.root);
    let observed = bob.expected(&library_copy);
    let before = library_copy.bytes();
    let mut copied_store = Store::open_existing(&library_copy.meta).unwrap();
    copied_store.bind_context(
        observed.instance.clone(),
        observed.mapping_revision,
        library_copy.root.clone(),
        None,
        library_copy.root.clone(),
        Default::default(),
    );
    let result = pipeline::commit_lifecycle(
        &mut copied_store,
        &mut NoProbe,
        &OsRng,
        &SystemClock,
        CommitKind::Delete,
        "doc.md",
        None,
        "",
        &observed,
    );
    assert!(result.is_err());
    drop(copied_store);
    assert_eq!(library_copy.bytes(), before);
}

#[test]
fn protect_cli_records_exact_planned_commit_and_distinct_link_identity() {
    let env = Env::new();
    let a = env.store("protect-consumer");
    let b = env.store("protect-target");
    init_range(&env, &a, "a.md", "abcdefghij");
    let (_, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);
    let planned_record = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let link_id = "planned-link-001";
    let protected = env.run(
        &b,
        &[
            "protect",
            &b_tip,
            "--peer",
            &a.info().store_id,
            "--record",
            planned_record,
            "--link",
            link_id,
        ],
    );
    let credential = protected["data"]["credential"].as_str().unwrap();
    assert_eq!(
        protected["data"]["record_id"].as_str(),
        Some(planned_record)
    );
    assert_eq!(protected["data"]["link_id"].as_str(), Some(link_id));
    let b_store = Store::open_existing(&b.meta).unwrap();
    let receipt = omd::records::cross::read_inbound(&b_store, credential).unwrap();
    assert_eq!(receipt.record_id, planned_record);
    assert_eq!(receipt.link_id, link_id);

    let gc = env.run(&b, &["gc"]);
    assert!(
        gc["data"]["collected_protections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(credential))
    );
}

#[test]
fn true_post_protection_failure_keeps_receipt_then_gc_proves_it_orphaned() {
    let env = Env::new();
    let _library_config = env.library_config();
    let a = env.store("a");
    let b = env.store("b");
    let (a_root, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (b_root, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);
    let expected = env.expected(&a);
    let peer_expected = expected.peers.get(&b.info().store_id).unwrap().clone();

    let mut a_store = Store::open_existing(&a.meta).unwrap();
    a_store.bind_context(
        expected.instance.clone(),
        expected.mapping_revision,
        a.root.clone(),
        None,
        a.root.clone(),
        Default::default(),
    );
    let mut b_store = Store::open_existing(&b.meta).unwrap();
    let mut fault = FaultInjector::new();
    fault.arm_once(PublishStage::WriteTempState, 1);
    let link_id = "latefail0011223344";
    let failure = pipeline::commit_xlink_protected(
        &mut a_store,
        &mut b_store,
        &mut fault,
        &OsRng,
        &SystemClock,
        &a_root,
        &a_root,
        &a_tip,
        &b.info().store_id,
        &b_root,
        &b_tip,
        link_id,
        "late failure",
        &expected,
        &peer_expected,
    )
    .unwrap_err();
    let credential = failure
        .credential_id
        .unwrap_or_else(|| panic!("protection is durable: {:?}", failure.error));
    drop(a_store);
    drop(b_store);
    assert!(!a.info().links.contains_key(link_id));
    assert!(b.info().inbound.contains_key(&credential));
    let b_store = Store::open_existing(&b.meta).unwrap();
    let receipt = omd::records::cross::read_inbound(&b_store, &credential).unwrap();
    assert_eq!(receipt.link_id, link_id);
    assert_ne!(receipt.record_id, link_id);
    assert!(
        a.meta
            .join(format!("commits/{}.toml", receipt.record_id))
            .is_file()
    );
    let a_store = Store::open_existing(&a.meta).unwrap();
    assert!(!a_store.state().retained.contains(&receipt.record_id));
    let planned = a_store.read_commit(&receipt.record_id).unwrap();
    assert_eq!(
        planned.payload["link_id"].as_str(),
        Some(receipt.link_id.as_str())
    );

    let gc = env.run(&b, &["gc", "--content"]);
    assert!(
        gc["data"]["collected_protections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(credential.as_str()))
    );
    assert!(!b.info().inbound.contains_key(&credential));
}

#[test]
fn offline_consumer_gc_retains_closure_but_unrelated_offline_peer_does_not_block_local_work() {
    let env = Env::new();
    let a = env.store("a");
    let b = env.store("b");
    let u = env.store("unrelated");
    let (_, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (_, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    init_range(&env, &u, "u.md", "uvwxyz0123");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);
    link_to_peer(&env, &a, "a.md", &a_tip, &b, &b_tip);
    env.register_peer(&b, &u);

    let a_hidden = a.root.with_extension("offline");
    let u_hidden = u.root.with_extension("offline");
    std::fs::rename(&a.root, &a_hidden).unwrap();
    std::fs::rename(&u.root, &u_hidden).unwrap();
    let gc = env.command(&b, &["gc", "--content"], true);
    assert_ok(&gc);
    let value = json(&gc);
    assert!(value["data"]["protected_count"].as_u64().unwrap() >= 1);
    assert!(
        value["data"]["protection_reasons"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );

    b.write("b.md", "klmnopqrst local");
    let b_current = b.info().ranges.values().next().unwrap().clone();
    let local = env.command(
        &b,
        &[
            "commit", "commit", "b.md", "--id", &b_current, "--range", "0", "4", "--reason",
            "local",
        ],
        true,
    );
    assert_ok(&local);
    std::fs::rename(&a_hidden, &a.root).unwrap();
    std::fs::rename(&u_hidden, &u.root).unwrap();
}

#[test]
fn grouped_store_endpoint_resolves_historical_and_current_versions() {
    let env = Env::new();
    let a = env.store("grouped-a");
    let b = env.store("grouped-b");
    let (_, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (b_root, historical) = init_range(&env, &b, "b.md", "klmnopqrst");
    env.run(
        &b,
        &[
            "commit",
            "commit",
            "b.md",
            "--id",
            &historical,
            "--range",
            "0",
            "5",
            "--reason",
            "advance",
        ],
    );
    let current = b.info().ranges[&b_root].clone();
    assert_ne!(historical, current);
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);

    let first = env.run(
        &a,
        &[
            "commit",
            "commit",
            "a.md",
            "--id",
            &a_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "historical",
            "--link-to-store",
            &b.info().store_id,
            &historical[..8],
        ],
    );
    let first_link = first["data"]["links"][0].as_str().unwrap();
    assert_eq!(
        a.info().links[first_link]["target_version"].as_str(),
        Some(historical.as_str())
    );

    let a_current = a.info().ranges.values().next().unwrap().clone();
    let second = env.run(
        &a,
        &[
            "commit",
            "commit",
            "a.md",
            "--id",
            &a_current,
            "--range",
            "0",
            "4",
            "--reason",
            "current",
            "--link-to-store",
            &b.info().store_id,
            &current,
        ],
    );
    let second_link = second["data"]["links"][0].as_str().unwrap();
    let links = a.info().links;
    assert_eq!(
        links[second_link]["target_version"].as_str(),
        Some(current.as_str())
    );
    assert_eq!(links[first_link]["target"], links[second_link]["target"]);
}

#[test]
fn grouped_store_endpoint_wrong_physical_identity_is_zero_write() {
    let env = Env::new();
    let a = env.store("wrong-a");
    let b = env.store("wrong-b");
    let wrong = env.store("wrong-selected");
    let (_, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (_, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    init_range(&env, &wrong, "wrong.md", "uvwxyz0123");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);
    let observed = serde_json::to_string(&env.expected(&a)).unwrap();
    let config_path = env.config.join("projects.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        config.replace(b.meta.to_str().unwrap(), wrong.meta.to_str().unwrap()),
    )
    .unwrap();
    let before_a = a.bytes();
    let before_b = b.bytes();
    let before_wrong = wrong.bytes();
    let output = env.command(
        &a,
        &[
            "--expected",
            &observed,
            "commit",
            "commit",
            "a.md",
            "--id",
            &a_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "wrong physical peer",
            "--link-to-store",
            &b.info().store_id,
            &b_tip,
        ],
        false,
    );
    assert!(!output.status.success());
    assert_eq!(a.bytes(), before_a);
    assert_eq!(b.bytes(), before_b);
    assert_eq!(wrong.bytes(), before_wrong);
}

#[test]
fn peer_lock_conflict_is_preflight_and_leaves_zero_publication() {
    let env = Env::new();
    let a = env.store("a");
    let b = env.store("b");
    let (_, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (_, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);
    let before_a = a.bytes();
    let before_b = b.bytes();
    let mut lock = fslock::LockFile::open(&b.meta.join("write.lock")).unwrap();
    lock.lock().unwrap();
    let output = env.command(
        &a,
        &[
            "commit",
            "commit",
            "a.md",
            "--id",
            &a_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "locked",
            "--link-to-store",
            &b.info().store_id,
            &b_tip,
        ],
        true,
    );
    drop(lock);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(json(&output)["diagnostics"][0]["kind"], "lock_conflict");
    assert_eq!(
        a.bytes(),
        before_a,
        "planned lock failure is pre-publication"
    );
    assert_eq!(b.bytes(), before_b, "no inbound receipt on lock failure");
}

#[test]
fn ordered_mixed_endpoints_resolve_aliases_and_reject_normalized_duplicates() {
    let env = Env::new();
    let local = env.store("local");
    let x = env.store("x");
    let y = env.store("y");
    let (a_root, _) = init_range(&env, &local, "a.md", "abcdefghij");
    let (b_root, b_tip) = init_range(&env, &local, "b.md", "klmnopqrst");
    let (c_root, _) = init_range(&env, &local, "c.md", "uvwxyz0123");
    let (x_root, x_historical) = init_range(&env, &x, "x.md", "ABCDEFGHIJ");
    let (_y_root, y_tip) = init_range(&env, &y, "y.md", "KLMNOPQRST");
    env.register_peer(&local, &x);
    env.register_peer(&x, &local);
    env.register_peer(&local, &y);
    env.register_peer(&y, &local);
    env.register_alias(&x, "x-one");
    env.register_alias(&x, "x-two");
    env.register_alias(&y, "y-one");

    let x_current = env.run(
        &x,
        &[
            "commit",
            "commit",
            "x.md",
            "--id",
            &x_historical,
            "--range",
            "0",
            "5",
            "--reason",
            "advance x",
        ],
    )["data"]["commit"]
        .as_str()
        .unwrap()
        .to_string();

    let mixed = env.run(
        &local,
        &[
            "commit",
            "commit",
            "b.md",
            "--id",
            &b_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "ordered",
            "--link-from",
            &a_root,
            "--link-to-store",
            "y-one",
            &y_tip,
            "--link-from-store",
            "x-one",
            &x_historical,
            "--link-to",
            &c_root,
        ],
    );
    let ordered: Vec<String> = mixed["data"]["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    assert_eq!(ordered.len(), 4);
    let info = local.info();
    let links: Vec<&toml::Value> = ordered.iter().map(|id| &info.links[id]).collect();
    assert_eq!(links[0]["source"].as_str(), Some(a_root.as_str()));
    assert_eq!(links[0]["target"].as_str(), Some(b_root.as_str()));
    assert_eq!(links[1]["source"].as_str(), Some(b_root.as_str()));
    assert!(
        links[1]["target"]
            .as_str()
            .unwrap()
            .contains(&y.info().store_id)
    );
    assert_eq!(links[1]["target_version"].as_str(), Some(y_tip.as_str()));
    assert!(
        links[2]["source"]
            .as_str()
            .unwrap()
            .contains(&x.info().store_id)
    );
    assert_eq!(
        links[2]["source_version"].as_str(),
        Some(x_historical.as_str())
    );
    assert_eq!(links[2]["target"].as_str(), Some(b_root.as_str()));
    assert_eq!(links[3]["source"].as_str(), Some(b_root.as_str()));
    assert_eq!(links[3]["target"].as_str(), Some(c_root.as_str()));
    assert!(x.info().inbound.len() >= 1);
    assert!(y.info().inbound.len() >= 1);

    let local_tip = local.info().ranges[&b_root].clone();
    let before_local = local.bytes();
    let before_x = x.bytes();
    let duplicate = env.command(
        &local,
        &[
            "commit",
            "commit",
            "b.md",
            "--id",
            &local_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "duplicate",
            "--link-from-store",
            "x-one",
            &x_current,
            "--link-from-store",
            "x-two",
            &x_current,
        ],
        true,
    );
    assert_eq!(duplicate.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&duplicate.stdout).contains("duplicate --link-from endpoint"),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&duplicate.stdout),
        String::from_utf8_lossy(&duplicate.stderr)
    );
    assert_eq!(
        local.bytes(),
        before_local,
        "duplicate preflight is zero-write"
    );
    assert_eq!(
        x.bytes(),
        before_x,
        "duplicate preflight writes no protection"
    );

    let opposite = env.run(
        &local,
        &[
            "commit",
            "commit",
            "b.md",
            "--id",
            &local_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "opposite directions",
            "--link-from-store",
            "x-one",
            &x_current,
            "--link-to-store",
            "x-two",
            &x_current,
        ],
    );
    assert_eq!(opposite["data"]["links"].as_array().unwrap().len(), 2);

    let later_tip = local.info().ranges[&b_root].clone();
    let later = env.run(
        &local,
        &[
            "commit",
            "commit",
            "b.md",
            "--id",
            &later_tip,
            "--range",
            "0",
            "4",
            "--reason",
            "same endpoint later",
            "--link-to-store",
            "x-one",
            &x_current,
        ],
    );
    assert_eq!(later["data"]["links"].as_array().unwrap().len(), 1);
    assert_eq!(x.info().ranges[&x_root], x_current);
}

#[test]
fn immutable_peer_and_inbound_snapshots_and_old_generation_zero_write_refusal() {
    let env = Env::new();
    let a = env.store("a");
    let b = env.store("b");
    let (_, a_tip) = init_range(&env, &a, "a.md", "abcdefghij");
    let (_, b_tip) = init_range(&env, &b, "b.md", "klmnopqrst");
    env.register_peer(&a, &b);
    env.register_peer(&b, &a);
    link_to_peer(&env, &a, "a.md", &a_tip, &b, &b_tip);

    let a_state: toml::Value =
        toml::from_str(&std::fs::read_to_string(a.meta.join("state.toml")).unwrap()).unwrap();
    let peer_revision = a_state["peers"][&b.info().store_id].as_str().unwrap();
    let peer_path = a.meta.join(format!("registrations/{peer_revision}.toml"));
    let peer_bytes = std::fs::read(&peer_path).unwrap();
    let inbound_id = b.info().inbound.keys().next().unwrap().clone();
    let inbound_path = b.meta.join(format!("inbound/{inbound_id}.toml"));
    let inbound_bytes = std::fs::read(&inbound_path).unwrap();
    env.register_peer(&a, &b);
    assert_eq!(std::fs::read(peer_path).unwrap(), peer_bytes);
    assert_eq!(std::fs::read(inbound_path).unwrap(), inbound_bytes);

    let before = a.bytes();
    let state_path = a.meta.join("state.toml");
    let old = std::fs::read_to_string(&state_path)
        .unwrap()
        .replace("format = \"omd.state/9\"", "format = \"omd.state/8\"");
    std::fs::write(&state_path, old).unwrap(); // explicit corruption test
    let after_corruption = a.bytes();
    let refused = env.command(&a, &["gc"], true);
    assert!(!refused.status.success());
    assert_eq!(
        a.bytes(),
        after_corruption,
        "old generation refusal must write nothing"
    );
    assert_ne!(
        before, after_corruption,
        "only explicit corruption changed bytes"
    );
}
