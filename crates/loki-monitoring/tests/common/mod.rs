//! Shared black-box test helper, mirroring
//! `crates/loki-storage/tests/common/mod.rs`'s pattern.
#![allow(dead_code)]

/// `SqliteTelemetryStore::open`/`RuleStore::open` go through
/// `loki_storage::open_or_migrate_sqlcipher`, which now requires the
/// process-global secure-memory arena to be ready first
/// (DAT-002-MEM-CODE-001, ADR-0005). Production never embeds an accepted
/// bounds profile at this revision (fail-closed by design), so this crate's
/// own black-box tests bootstrap one disposable, non-production test-only
/// arena per test binary instead — see
/// `loki_storage::secure_memory::test_support` for why this bypass cannot
/// reach a release build.
pub fn ensure_ready() {
    loki_storage::secure_memory::test_support::ensure_test_arena_ready();
}
