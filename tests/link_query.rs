//! `omd links` query surface: summary-first, filtered detail, paging
//! contract, single-link mode — through the real binary.

mod common;
use std::process::Command;

static TDIR_UNIQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn omd() -> std::path::PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_omd") {
        return p.into();
    }
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    p.pop();
    p.push("omd");
    p
}

struct T(std::path::PathBuf);
impl T {
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!(
            "omd-lq-{}-{}",
            std::process::id(),
            TDIR_UNIQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&r).unwrap();
        Self(r)
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = Command::new(omd())
            .arg("--meta")
            .arg(self.0.join(".omd"))
            .args(common::with_expected(
                &omd(),
                &self.0,
                args,
                Some(&self.0.join(".omd")),
                Some(&self.0.join("home")),
                Some(&self.0.join("config.toml")),
                Some(&self.0.join("cache")),
            ))
            .current_dir(&self.0)
            .env("HOME", self.0.join("home"))
            .env("OMD_CONFIG_PATH", self.0.join("config.toml"))
            .env("OMD_CACHE_PATH", self.0.join("cache"))
            .output()
            .unwrap();
        (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).into(),
            String::from_utf8_lossy(&o.stderr).into(),
        )
    }
    fn write(&self, p: &str, c: &str) {
        let f = self.0.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, c).unwrap();
    }
    fn json(out: &str) -> serde_json::Value {
        serde_json::from_str(out).expect("stdout is JSON")
    }
    /// Node keys of the ranges mounted under `file` (e.g. "a.md").
    fn ranges_of(&self, file: &str) -> Vec<String> {
        let (_, out, _) = self.run(&["tree", "--json"]);
        T::json(&out)["data"]["tree"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["path"].as_str() == Some(file))
            .flat_map(|f| f["children"].as_array().cloned().unwrap_or_default())
            .map(|r| r["node"].as_str().unwrap_or_default().to_string())
            .collect()
    }
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Two files, one confirmed range each, linked A→B. Returns (source node,
/// target node, link_id).
fn linked_pair(t: &T) -> (String, String, String) {
    t.write("a.md", "aaaa\n");
    t.write("b.md", "bbbb\n");
    t.run(&["init", "a.md"]);
    t.run(&["init", "b.md"]);
    t.run(&["commit", "commit", "a.md", "--range", "0", "4"]);
    t.run(&["commit", "commit", "b.md", "--range", "0", "4"]);
    let ra = t.ranges_of("a.md").remove(0);
    let rb = t.ranges_of("b.md").remove(0);
    let (c, out, err) = t.run(&["commit", "link", "a.md", "--source", &ra, "--target", &rb]);
    assert_eq!(c, 0, "{err}");
    let lid = T::json(&out)["data"]["link_id"]
        .as_str()
        .expect("link id")
        .to_string();
    (ra, rb, lid)
}

#[test]
fn summary_has_no_items_regardless_of_store() {
    let t = T::new();
    t.write("a.md", "aaaa\n");
    t.run(&["init", "a.md"]);
    let (c, out, _) = t.run(&["links", "--json"]);
    assert_eq!(c, 0, "{out}");
    let data = &T::json(&out)["data"];
    assert!(data.get("items").is_none(), "summary must not carry items");
    assert_eq!(data["total"], 0);
    assert!(data["by_status"]["healthy"].is_u64());
    assert!(data["by_stratum"].is_object());
}

#[test]
fn summary_rejects_detail_modifiers_as_usage_errors() {
    let t = T::new();
    linked_pair(&t);
    // Orphan flags without `list`/`show` are usage errors, never silently
    // ignored against a summary.
    for args in [
        vec!["links", "--limit", "5"],
        vec!["links", "--full"],
        vec!["links", "--status", "healthy"],
    ] {
        let (c, out, err) = t.run(&args);
        assert_ne!(c, 0, "{args:?} must fail: {out} {err}");
    }
}

#[test]
fn unfiltered_list_enumerates_all_in_pages() {
    let t = T::new();
    // Two independent links — no filter means everything shows up.
    linked_pair(&t);
    t.write("m.md", "0123456789\n");
    t.run(&["init", "m.md"]);
    for i in 0..2u64 {
        t.run(&[
            "commit",
            "commit",
            "m.md",
            "--range",
            &i.to_string(),
            &(i + 1).to_string(),
        ]);
    }
    let ranges = t.ranges_of("m.md");
    t.run(&[
        "commit", "link", "m.md", "--source", &ranges[0], "--target", &ranges[1],
    ]);
    let (c, out, _) = t.run(&["links", "list", "--json"]);
    assert_eq!(c, 0, "{out}");
    let data = &T::json(&out)["data"];
    assert_eq!(data["total"], 2, "unfiltered list = full enumeration");
    assert_eq!(data["items"].as_array().unwrap().len(), 2);
    assert_eq!(data["has_more"], false);
}

#[test]
fn status_filter_lists_matching_links_only() {
    let t = T::new();
    linked_pair(&t);
    let (c, out, _) = t.run(&["links", "list", "--json", "--status", "healthy"]);
    assert_eq!(c, 0, "{out}");
    let data = &T::json(&out)["data"];
    assert_eq!(data["total"], 1, "expected the linked pair: {data}");
    assert_eq!(data["items"].as_array().unwrap().len(), 1);
    let item = &data["items"][0];
    // Brief shape: exactly the five bounded fields.
    let obj = item.as_object().unwrap();
    assert_eq!(obj.len(), 5, "brief must stay five fields: {obj:?}");
    assert_eq!(item["status"], "healthy");
    // A filter that matches nothing yields a valid empty page.
    let (_, xout, _) = t.run(&["links", "list", "--json", "--status", "broken"]);
    let xd = &T::json(&xout)["data"];
    assert_eq!(xd["total"], 0);
    assert_eq!(xd["items"].as_array().unwrap().len(), 0);
    assert_eq!(xd["has_more"], false);
}

#[test]
fn node_filter_is_bidirectional() {
    let t = T::new();
    let (ra, rb, _) = linked_pair(&t);
    for node in [&ra, &rb] {
        let (_, nout, _) = t.run(&["links", "list", "--json", "--node", node]);
        let nd = &T::json(&nout)["data"];
        assert_eq!(nd["total"], 1, "node {node} must match both directions");
    }
    // A non-matching node matches nothing.
    let (_, xout, _) = t.run(&["links", "list", "--json", "--node", "range:deadbeef"]);
    assert_eq!(T::json(&xout)["data"]["total"], 0);
}

#[test]
fn single_link_full_detail_excludes_others() {
    let t = T::new();
    let (_, _, lid) = linked_pair(&t);
    let (c, out, _) = t.run(&["links", "show", &lid, "--json"]);
    assert_eq!(c, 0, "{out}");
    let data = &T::json(&out)["data"];
    assert!(
        data.get("items").is_none(),
        "single-link mode must not page"
    );
    let link = &data["link"];
    assert_eq!(link["link_id"], lid);
    assert_eq!(link["status"], "healthy");
    assert!(
        link["full"]["source"]["object"].is_object(),
        "full projection embedded"
    );
    assert!(
        link["full"]["target"]["object"].is_object(),
        "full projection embedded"
    );
    // Unknown id is a usage error, not a crash.
    let (c2, _, _) = t.run(&["links", "show", "no-such-link", "--json"]);
    assert_eq!(c2, 2);
}

#[test]
fn paging_contract_limits_and_cursors() {
    let t = T::new();
    // One file, six confirmed ranges, chained links r0->r1->...->r5.
    t.write("m.md", "0123456789\n");
    t.run(&["init", "m.md"]);
    for i in 0..6u64 {
        t.run(&[
            "commit",
            "commit",
            "m.md",
            "--range",
            &i.to_string(),
            &(i + 1).to_string(),
        ]);
    }
    let ranges = t.ranges_of("m.md");
    assert_eq!(ranges.len(), 6, "six ranges: {ranges:?}");
    for pair in ranges.windows(2) {
        let (c, _, err) = t.run(&[
            "commit", "link", "m.md", "--source", &pair[0], "--target", &pair[1],
        ]);
        assert_eq!(c, 0, "{err}");
    }
    let (c, out, _) = t.run(&["links", "list", "--json", "--status", "healthy"]);
    assert_eq!(c, 0);
    let total = T::json(&out)["data"]["total"].as_u64().unwrap();
    assert_eq!(total, 5, "five chained links");

    // Small page + cursor walk, no overlap.
    let (_, p1, _) = t.run(&[
        "links", "list", "--json", "--status", "healthy", "--limit", "2",
    ]);
    let d1 = &T::json(&p1)["data"];
    assert_eq!(d1["total"], total);
    assert_eq!(d1["items"].as_array().unwrap().len(), 2);
    assert_eq!(d1["has_more"], true);
    let next = d1["next"].as_str().unwrap().to_string();
    let (_, p2, _) = t.run(&[
        "links", "list", "--json", "--status", "healthy", "--limit", "2", "--cursor", &next,
    ]);
    let d2 = &T::json(&p2)["data"];
    assert_eq!(d2["offset"], 2);
    let ids1: Vec<&str> = d1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["link_id"].as_str().unwrap())
        .collect();
    let ids2: Vec<&str> = d2["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["link_id"].as_str().unwrap())
        .collect();
    assert!(ids1.iter().all(|id| !ids2.contains(id)), "pages overlap");

    // Oversized limit caps at 100 with a note.
    let (_, big, _) = t.run(&[
        "links", "list", "--json", "--status", "healthy", "--limit", "500",
    ]);
    let db = &T::json(&big)["data"];
    assert_eq!(db["limit"], 100);
    assert!(db.get("note").is_some(), "cap note missing");
}

#[test]
fn stratum_filter_partitions_links() {
    let t = T::new();
    linked_pair(&t);
    let (_, out, _) = t.run(&["links", "--json"]);
    let data = &T::json(&out)["data"];
    let total: u64 = data["total"].as_u64().unwrap();
    let sum: u64 = data["by_stratum"]
        .as_object()
        .unwrap()
        .values()
        .filter_map(|v| v.as_u64())
        .sum();
    assert_eq!(sum, total, "by_stratum must partition all links");
    // Each stratum queried separately reconstitutes the same total.
    let mut seen = 0u64;
    for (k, v) in data["by_stratum"].as_object().unwrap() {
        let (_, sout, _) = t.run(&["links", "list", "--json", "--stratum", k]);
        let sd = &T::json(&sout)["data"];
        assert_eq!(sd["total"], *v, "stratum {k} mismatch");
        seen += sd["total"].as_u64().unwrap();
    }
    assert_eq!(seen, total);
}
