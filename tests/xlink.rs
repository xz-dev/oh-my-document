//! Cross-store behavior moved to `tests/cross_store.rs`.
//!
//! S3 requires real mutual peer registration, caller-observed peer evidence,
//! canonical ordered locks, immutable inbound receipts, and copy authority.
//! The former fixtures used shared absolute locators and asserted success even
//! when registration failed, so they were replaced rather than adapted.
