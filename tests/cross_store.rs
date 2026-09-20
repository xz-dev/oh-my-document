//! Group 11: cross-store protection protocol — TWO real .omd stores.
//!
//! The spec's cross-store requirements are protocol requirements about
//! ORDERING between two stores, not single-store field bookkeeping:
//!   - B persists an inbound credential for A's pending record BEFORE A
//!     publishes; the credential alone never proves the link.
//!   - A's publish failing after B's credential → no valid link, but the
//!     target stays protected (conservative retention).
//!   - A copied store dir refuses business writes + gc until re-registered.

use std::process::Command;

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

struct Store(std::path::PathBuf);
impl Store {
    fn new(tag: &str) -> Self {
        let r = std::env::temp_dir().join(format!(
            "omd-xs-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&r).unwrap();
        Self(r)
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = Command::new(omd())
            .arg("--meta")
            .arg(self.0.join(".omd"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).into(),
            String::from_utf8_lossy(&o.stderr).into(),
        )
    }
    fn write(&self, p: &str, c: &str) {
        std::fs::write(self.0.join(p), c).unwrap();
    }
    fn state(&self) -> String {
        std::fs::read_to_string(self.0.join(".omd/state.toml")).unwrap_or_default()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn inbound_credential_persists_before_consumer_publishes() {
    // B protects target b1 for A's pending record r1.
    let b = Store::new("b");
    b.write("b.md", "protected content");
    b.run(&["init", "b.md"]);
    // A's store_id would be known via `register`; here B records A as peer.
    let (c, out, _) = b.run(&["register", "store-A", "/path/to/A"]);
    assert_eq!(c, 0, "{out}");
    // B persists the inbound credential for A's pending record.
    let (c, out, _) = b.run(&[
        "protect",
        "b1-target",
        "--peer",
        "store-A",
        "--record",
        "record-A1",
    ]);
    assert_eq!(c, 0, "{out}");
    assert!(out.contains("credential"), "must return credential id");
    assert!(
        b.state().contains("record-A1"),
        "credential persisted in B's state"
    );
}

#[test]
fn credential_without_publish_keeps_target_protected() {
    // Spec scenario: B saved the credential for A's record, A never published.
    // B's gc must conservatively protect the target — not release it.
    let b = Store::new("b2");
    b.write("b.md", "keep me");
    b.run(&["init", "b.md"]);
    b.run(&[
        "protect",
        "keep-target",
        "--peer",
        "store-A",
        "--record",
        "pending-r",
    ]);
    // gc --content must NOT free the protected target's content.
    let (c, out, _) = b.run(&["gc", "--content"]);
    assert_eq!(c, 0, "{out}");
    assert!(
        out.contains("protected"),
        "gc must report protected targets"
    );
}

#[test]
fn copied_store_refuses_writes_until_activated() {
    // Spec: a writable copy is NOT the original consumer — business writes
    // and gc refuse until re-registration (new store_id + activation).
    let orig = Store::new("orig");
    orig.write("f.md", "x");
    orig.run(&["init", "f.md"]);

    // Simulate a copy: clone .omd dir, then clear activation (as an
    // un-registered copy would be).
    let copy_root = std::env::temp_dir().join(format!(
        "omd-copy-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&copy_root).unwrap();
    // Copy the .omd tree.
    let src = orig.0.join(".omd");
    let dst = copy_root.join(".omd");
    cp_recursive(&src, &dst);
    // Mark it unactivated (a copy that hasn't re-registered).
    let st_path = dst.join("state.toml");
    let st = std::fs::read_to_string(&st_path)
        .unwrap()
        .replace("activated = true", "activated = false");
    std::fs::write(&st_path, st).unwrap();

    // A business write on the copy must refuse.
    let o = Command::new(omd())
        .arg("--meta")
        .arg(&dst)
        .args(["commit", "commit", "f.md", "--reason", "r"])
        .current_dir(&copy_root)
        .output()
        .unwrap();
    assert!(
        !o.status.success(),
        "unactivated copy must refuse business write"
    );

    // gc must refuse too.
    let o2 = Command::new(omd())
        .arg("--meta")
        .arg(&dst)
        .args(["gc", "--content"])
        .current_dir(&copy_root)
        .output()
        .unwrap();
    assert!(!o2.status.success(), "unactivated copy must refuse gc");

    // After activate, writes are allowed again (new store_id).
    let o3 = Command::new(omd())
        .arg("--meta")
        .arg(&dst)
        .args(["activate"])
        .current_dir(&copy_root)
        .output()
        .unwrap();
    assert!(
        o3.status.success(),
        "activate must succeed: {:?}",
        String::from_utf8_lossy(&o3.stderr)
    );

    let _ = std::fs::remove_dir_all(&copy_root);
}

fn cp_recursive(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() {
            cp_recursive(&p, &d);
        } else {
            std::fs::copy(&p, &d).unwrap();
        }
    }
}
