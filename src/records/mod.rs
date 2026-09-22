//! Records and storage: immutable commits, source versions, bindings,
//! registrations, state publication, locks, expected-version checks,
//! notes, inbound receipts, pending material, and garbage collection.

pub mod commit;
pub mod id;
pub mod ids;
pub mod ops;
pub mod pipeline;
pub mod registration;
pub mod store;
pub mod time;
pub mod version;

pub mod binding;
pub mod cross;
pub mod notes;
#[cfg(test)]
mod tests;
