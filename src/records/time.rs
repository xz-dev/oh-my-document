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

/// A clock pinned to a user-supplied RFC3339 timestamp — manual replay.
/// `--timestamp <rfc3339>` lets a user re-commit with a recorded time to
/// resolve a conflict by ordering; the commit's canonical timestamp field
/// uses it, so the id derivation includes it like any other commit.
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Parse an RFC3339 timestamp into a nanosecond Timestamp. Accepts the same
/// format `format_timestamp` emits.
pub fn parse_rfc3339(s: &str) -> Option<Timestamp> {
    // Minimal RFC3339: `YYYY-MM-DDTHH:MM:SS[.nsec]Z`.
    let s = s.trim();
    let (date_time, frac) = match s.split_once('.') {
        Some((d, f)) => (d, f.trim_end_matches('Z')),
        None => (s.trim_end_matches('Z'), ""),
    };
    let (date, time) = date_time.split_once('T')?;
    let (y, mo, d): (i64, i64, i64) = {
        let mut it = date.split('-');
        (
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
        )
    };
    let (h, mi, se): (i64, i64, i64) = {
        let mut it = time.split(':');
        (
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
        )
    };
    // Strict validation — never normalize an invalid date into a real one.
    // --timestamp is a manual replay tool: the recorded id derives from the
    // recorded value, so silent re-interpretation breaks replay determinism.
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = match mo {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return None, // month 0 or 13+
    };
    if !(1..=9999).contains(&y)
        || !(1..=dim).contains(&d)
        || !(0..=23).contains(&h)
        || !(0..=59).contains(&mi)
        || !(0..=59).contains(&se)
    {
        return None;
    }
    // Days since epoch (civil) — Howard Hinnant algorithm.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + (h * 3600 + mi * 60 + se);
    let nanos: u64 = format!("{:0<9}", frac).parse().ok()?;
    Some(Timestamp(secs as u64 * 1_000_000_000 + nanos))
}

/// OS CSPRNG for salts and ids. `FixedRng` is deterministic/test-only; this
/// is what production uses so salts are unpredictable per the contract.
pub struct OsRng;

impl Rng for OsRng {
    fn fill(&self, out: &mut [u8]) {
        getrandom::fill(out).expect("OS RNG unavailable");
    }
}
