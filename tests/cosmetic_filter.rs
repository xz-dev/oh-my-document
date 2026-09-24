//! `verify/check --difftastic` cosmetic filtering and `commit cosmetic`
//! batch finishing — end-to-end through the real binary and a real `difft`.
//! Skips cleanly when `difft` is not on PATH (CI without the tool still
//! exercises everything else).

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

fn difft_available() -> bool {
    Command::new("difft")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

struct T(std::path::PathBuf);
impl T {
    fn new() -> Self {
        let r = std::env::temp_dir().join(format!(
            "omd-cos-{}-{}",
            std::process::id(),
            TDIR_UNIQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&r).unwrap();
        Self(r)
    }
    /// Run `omd <args>` with context flags; `expected` evidence is acquired
    /// automatically by `with_expected` for write operations.
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
}
impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A formatting-only change lands the range in `cosmetic`, not `dirty`.
#[test]
fn formatting_storm_filters_to_cosmetic() {
    if !difft_available() {
        eprintln!("difft not on PATH — skipping");
        return;
    }
    let t = T::new();
    t.write("m.rs", "fn a() {\n    let x = 1;\n    let y = 2;\n}\n");
    t.run(&["init", "m.rs"]);
    // Track the whole body.
    t.run(&["commit", "commit", "m.rs", "--range", "0", "41"]);
    // Reformat whitespace only — structure unchanged.
    t.write("m.rs", "fn a() {\n    let x  =  1;\n    let y  =  2;\n}\n");

    let (c, out, _) = t.run(&["verify", "--difftastic", "--json"]);
    assert_eq!(c, 0, "cosmetic-only must exit 0: {out}");
    let d = T::json(&out);
    let data = &d["data"];
    assert!(
        data["dirty"].as_object().unwrap().is_empty(),
        "dirty must be empty: {}",
        data["dirty"]
    );
    let cosmetic = data["cosmetic"].as_object().expect("cosmetic bucket");
    assert_eq!(cosmetic.len(), 1, "one cosmetic range: {cosmetic:?}");
    let ev = &cosmetic.values().next().unwrap()[0]["evidence"];
    assert_eq!(ev["status"], "unchanged");
    assert!(ev["tool"].as_str().unwrap().contains("ifft"));

    // Without the flag the same content is still dirty — view ≠ state.
    let (c2, out2, _) = t.run(&["verify", "--json"]);
    assert_eq!(c2, 1, "unfiltered verify still fails");
    assert!(
        !T::json(&out2)["data"]["dirty"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

/// A structural change stays in `dirty` even under `--difftastic`.
#[test]
fn logic_change_stays_dirty() {
    if !difft_available() {
        eprintln!("difft not on PATH — skipping");
        return;
    }
    let t = T::new();
    t.write("m.rs", "fn a() {\n    let x = 1;\n}\n");
    t.run(&["init", "m.rs"]);
    t.run(&["commit", "commit", "m.rs", "--range", "0", "25"]);
    t.write("m.rs", "fn a() {\n    let x = 99;\n}\n"); // value change

    let (c, out, _) = t.run(&["verify", "--difftastic", "--json"]);
    assert_eq!(c, 1);
    let d = T::json(&out);
    assert!(!d["data"]["dirty"].as_object().unwrap().is_empty());
    assert!(
        d["data"]["cosmetic"].is_null() || d["data"]["cosmetic"].as_object().unwrap().is_empty()
    );
}

/// `commit cosmetic` finishes cosmetic ranges so unfiltered verify passes.
#[test]
fn commit_cosmetic_finishes_storm() {
    if !difft_available() {
        eprintln!("difft not on PATH — skipping");
        return;
    }
    let t = T::new();
    t.write("m.rs", "fn a() {\n    let x = 1;\n    let y = 2;\n}\n");
    t.run(&["init", "m.rs"]);
    t.run(&["commit", "commit", "m.rs", "--range", "0", "41"]);
    t.write("m.rs", "fn a() {\n    let x  =  1;\n    let y  =  2;\n}\n");

    let (c, out, err) = t.run(&["commit", "cosmetic", "m.rs"]);
    assert_eq!(c, 0, "cosmetic commit failed: {err}\n{out}");
    let d = T::json(&out);
    let finished = d["data"]["cosmetic_finished"]
        .as_array()
        .expect("finished list");
    assert!(!finished.is_empty(), "expected ≥1 finished range");

    // Unfiltered verify now passes — the sweep is a real confirmation.
    let (c2, out2, _) = t.run(&["verify", "--json"]);
    assert_eq!(c2, 0, "post-cosmetic verify should pass: {out2}");
}

/// Content changed between filtering and finishing is refused.
///
/// The credential pins the classified version: editing the file after the
/// `verify --difftastic` acquisition makes the expected evidence stale, so
/// the write is rejected at the credential boundary (version_conflict)
/// before any range is continued — the classified content is never the
/// confirmed content when they diverge.
#[test]
fn stale_cosmetic_view_is_refused() {
    if !difft_available() {
        eprintln!("difft not on PATH — skipping");
        return;
    }
    let t = T::new();
    t.write("m.rs", "fn a() {\n    let x = 1;\n}\n");
    t.run(&["init", "m.rs"]);
    t.run(&["commit", "commit", "m.rs", "--range", "0", "25"]);
    t.write("m.rs", "fn a() {\n    let x  =  1;\n}\n"); // cosmetic

    // Acquire a credential pinned to the cosmetic view, THEN change the
    // file again to a real logic edit before finishing.
    let (_, vout, _) = t.run(&["verify", "--difftastic", "--json"]);
    let expected = T::json(&vout)["data"]["expected"].to_string();
    t.write("m.rs", "fn a() {\n    let x  =  99;\n}\n"); // now structural

    let (c, out, _) = t.run(&[
        "commit",
        "cosmetic",
        "m.rs",
        "--expected",
        &expected,
        "--json",
    ]);
    // Refused — either as a stale credential (version_conflict, exit 3) or
    // as a re-classification that found real structure change (exit 1).
    // Both honor "never publish from an outdated filtered view".
    assert!(c == 1 || c == 3, "stale cosmetic must refuse: {out}");
    assert!(
        out.contains("structurally changed") || out.contains("version conflict"),
        "{out}"
    );
}

/// A missing `difft` keeps ranges in dirty with an honest diagnostic.
#[test]
fn missing_difft_is_unclassified_not_cosmetic() {
    let t = T::new();
    t.write("m.rs", "fn a() {\n    let x = 1;\n}\n");
    t.run(&["init", "m.rs"]);
    t.run(&["commit", "commit", "m.rs", "--range", "0", "25"]);
    t.write("m.rs", "fn a() {\n    let x  =  1;\n}\n");

    let o = Command::new(omd())
        .arg("--meta")
        .arg(t.0.join(".omd"))
        .args(["verify", "--difftastic", "--json"])
        .current_dir(&t.0)
        .env("HOME", t.0.join("home"))
        .env("OMD_CONFIG_PATH", t.0.join("config.toml"))
        .env("OMD_CACHE_PATH", t.0.join("cache"))
        .env("PATH", "/nonexistent") // difft unreachable
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let d: serde_json::Value = serde_json::from_str(&out).expect("json envelope survives");
    assert!(!d["data"]["dirty"].as_object().unwrap().is_empty());
    let msgs = d["data"]["dirty"].to_string();
    assert!(msgs.contains("unclassified"), "{msgs}");
    assert!(o.status.code() == Some(1));
}
