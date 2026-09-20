//! Test harness shared by unit and integration tests.
//!
//! Provides the four seams the design requires to make behavior observable
//! and injectable without product code depending on the clock, OS randomness,
//! real subprocesses, or real filesystem failure points:
//!
//! - [`Sandbox`]: an isolated temp directory holding both source files and a
//!   metadata store, deleted on drop.
//! - [`Clock`]/[`FixedClock`]: deterministic time for record timestamps.
//! - [`Rng`]/[`FixedRng`]: deterministic bytes for salts and 128-bit IDs.
//! - [`FaultInjector`]: named publication stages that can be armed to fail
//!   exactly once at a chosen step, so interrupted-write tests are explicit
//!   rather than implicit.
//! - [`OmdCmd`]: runs the real `omd` binary against a sandbox so CLI tests
//!   exercise the same path a user would.

use std::cell::Cell;
use std::collections::HashSet;
use std::env;
use std::sync::OnceLock;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Fixed-point nanosecond timestamp used by `FixedClock`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub u64);

/// Time source for record timestamps.
pub trait Clock {
    fn now(&self) -> Timestamp;
}

/// Clock that always returns a fixed timestamp; each call can step forward.
pub struct FixedClock {
    start: Timestamp,
    step_nanos: u64,
    tick: Cell<u64>,
}

impl FixedClock {
    pub fn new(start: Timestamp) -> Self {
        Self { start, step_nanos: 1, tick: Cell::new(0) }
    }
    pub fn with_step(mut self, step_nanos: u64) -> Self {
        self.step_nanos = step_nanos;
        self
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        let t = self.tick.get();
        self.tick.set(t + 1);
        Timestamp(self.start.0 + t * self.step_nanos)
    }
}

/// Byte source for salts and 128-bit IDs.
pub trait Rng {
    fn fill(&self, out: &mut [u8]);
}

/// Deterministic byte generator (xorshift) — not cryptographic, test-only.
pub struct FixedRng {
    state: Cell<u64>,
}

impl FixedRng {
    pub fn new(seed: u64) -> Self {
        Self { state: Cell::new(seed.max(1)) }
    }
}

impl Rng for FixedRng {
    fn fill(&self, out: &mut [u8]) {
        let mut s = self.state.get();
        for byte in out.iter_mut() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *byte = (s & 0xff) as u8;
        }
        self.state.set(s);
    }
}

/// Re-export the production stage enum so tests arm real boundaries.
pub use crate::records::store::Stage as PublishStage;

/// Records each reached stage; arms one stage to fail at a chosen call index.
/// Implements the production `PublishProbe` seam — library code never
/// imports this module; the probe is passed in by the caller.
#[derive(Default)]
pub struct FaultInjector {
    armed_at: Option<(PublishStage, usize)>,
    pub reached: HashSet<PublishStage>,
}

impl FaultInjector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fail `stage` the `n`-th time it is reached (1-based).
    pub fn arm_once(&mut self, stage: PublishStage, n: usize) {
        self.armed_at = Some((stage, n));
    }

    /// Call at each stage boundary. Returns `true` when the stage must fail.
    pub fn hit(&mut self, stage: PublishStage) -> bool {
        self.reached.insert(stage);
        match self.armed_at {
            Some((s, n)) if s == stage => {
                let n = n.saturating_sub(1);
                if n == 0 {
                    self.armed_at = None;
                    return true;
                }
                self.armed_at = Some((stage, n));
                false
            }
            _ => false,
        }
    }
}

impl crate::records::store::PublishProbe for FaultInjector {
    fn at(&mut self, stage: PublishStage) -> bool {
        !self.hit(stage)
    }
}

/// Isolated test sandbox: one temp dir containing `src/` (source files) and
/// `meta/` (metadata store). Removed on drop.
pub struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    pub fn new() -> std::io::Result<Self> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("src"))?;
        std::fs::create_dir_all(root.path().join("meta"))?;
        Ok(Self { root })
    }

    pub fn source_dir(&self) -> PathBuf {
        self.root.path().join("src")
    }
    pub fn meta_dir(&self) -> PathBuf {
        self.root.path().join("meta")
    }
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Write a source file, creating parent dirs. Returns its path.
    pub fn write_source(&self, rel: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let p = self.source_dir().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, bytes)?;
        Ok(p)
    }
    pub fn write_source_str(&self, rel: &str, text: &str) -> std::io::Result<PathBuf> {
        self.write_source(rel, text.as_bytes())
    }
    pub fn read_source(&self, rel: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.source_dir().join(rel))
    }
}

/// Path to the built `omd` binary, resolved once.
fn omd_binary() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        if let Ok(p) = env::var("CARGO_BIN_EXE_omd") {
            return PathBuf::from(p);
        }
        // Fallback: derive from the test binary's location
        // (target/<profile>/deps/<test>) → target/<profile>/omd.
        let exe = env::current_exe().expect("current exe");
        let profile_dir = exe
            .parent()
            .and_then(|d| d.parent())
            .expect("profile dir");
        profile_dir.join(if cfg!(windows) { "omd.exe" } else { "omd" })
    })
    .clone()
}

/// Runs the real `omd` binary in a sandbox.
pub struct OmdCmd {
    sandbox: std::rc::Rc<Sandbox>,
    args: Vec<String>,
}

impl OmdCmd {
    pub fn in_sandbox(sandbox: std::rc::Rc<Sandbox>) -> Self {
        Self { sandbox, args: Vec::new() }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }
    pub fn args<I, S>(mut self, more: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in more {
            self.args.push(a.as_ref().to_string_lossy().into_owned());
        }
        self
    }

    pub fn run(&self) -> std::io::Result<Output> {
        let bin = omd_binary();
        Command::new(bin)
            .args(&self.args)
            .current_dir(self.sandbox.source_dir())
            .output()
    }

    /// Run and return stdout as text on success.
    pub fn succeed(&self) -> String {
        let out = self.run().expect("spawn omd");
        assert!(
            out.status.success(),
            "omd {:?} failed: {}",
            self.args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Run and return the process exit code + stderr on expected failure.
    pub fn fail(&self) -> (i32, String) {
        let out = self.run().expect("spawn omd");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}
