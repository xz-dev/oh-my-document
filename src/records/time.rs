//! Production time + entropy sources.
//!
//! `SystemClock` yields real UTC nanoseconds; `OsRng` pulls from the OS
//! CSPRNG (`getrandom`) for salts and 128-bit IDs. These are the only
//! non-deterministic inputs in the whole pipeline — everything else is
//! derived or stored.

use crate::testing::{Clock, Rng, Timestamp};

/// Real clock: nanoseconds since the Unix epoch, UTC.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp(d.as_secs() * 1_000_000_000 + d.subsec_nanos() as u64)
    }
}

/// OS CSPRNG for salts and ids. `FixedRng` is deterministic/test-only; this
/// is what production uses so salts are unpredictable per the contract.
pub struct OsRng;

impl Rng for OsRng {
    fn fill(&self, out: &mut [u8]) {
        getrandom::fill(out).expect("OS RNG unavailable");
    }
}
