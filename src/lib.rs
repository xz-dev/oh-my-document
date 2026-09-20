//! OMD core library: records/storage, sources/diff, relations/checks.
//! The CLI crate surface stays thin; all behavior lives behind this library.

pub mod records;
pub mod relations;
pub mod sources;

/// Shared test harness: temp isolation, controllable clock/RNG,
/// subprocess runner, and publication-stage fault injection.
/// Hidden from the public API; consumed by unit and integration tests.
#[doc(hidden)]
pub mod testing;
