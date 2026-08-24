//! Shared black-box test helpers. `tests/common/mod.rs` (a subdirectory, not
//! a top-level `tests/*.rs` file) is Cargo's documented convention for
//! integration-test-only code shared across multiple test binaries without
//! itself becoming a separate test binary.
//!
//! Each `tests/*.rs` file is compiled as its own independent crate with its
//! own copy of this module, and each one only uses a subset of the helpers
//! below — `#![allow(dead_code)]` avoids a per-binary false-positive
//! "never used" warning for whichever helpers a given test binary happens
//! not to call, which is expected for a shared-fixture module, not a real
//! unused-code issue.
#![allow(dead_code)]

use loki_storage::devices::NewDevice;
use loki_storage::ids::{ConnectorId, CorrelationId, DeviceId, JobId, RuntimeId};
use loki_storage::jobs::NewJob;
use loki_storage::states::{DeviceLifecycleState, JobState, SupportTier};
use loki_storage::{Cipher, Storage};

/// A fixed, deterministic timestamp for tests that don't care about a
/// specific value — this crate never generates timestamps itself (see
/// `src/timestamp.rs`), so every test must supply one explicitly.
pub const T0: &str = "2026-01-01T00:00:00Z";
pub const T1: &str = "2026-01-02T00:00:00Z";

pub fn cipher_for(path: &std::path::Path) -> Cipher {
    let key_path = path.with_extension("key.protected");
    let key = loki_storage::secret::load_or_create_database_key(&key_path, false)
        .expect("create/load DPAPI-protected test key");
    Cipher::SqlCipher(key)
}

/// Opens a fresh file-backed `Storage` in a new temporary directory (kept
/// alive by the returned `TempDir` — dropping it deletes the directory) and
/// applies every migration. Returns the temp dir alongside the DB path so
/// callers that need to close and reopen `Storage` (restart-style tests)
/// can do so against the same path.
pub fn open_temp_storage() -> (tempfile::TempDir, std::path::PathBuf, Storage) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("loki-storage-test.db");
    let storage = Storage::open(&path, cipher_for(&path)).expect("open encrypted storage");
    storage.run_migrations(T0).expect("run migrations");
    (dir, path, storage)
}

pub fn sample_device(id: &str) -> NewDevice {
    // Field values below are deliberately generic/synthetic test fixture
    // data, not a real connector or hardware claim — the certified hardware
    // target (AGENTS.md "Initial certified hardware target") is not yet
    // confirmed, and this crate is device-family-neutral by design (it
    // stores whatever opaque strings a caller supplies without interpreting
    // them).
    NewDevice {
        device_id: DeviceId::new(id).unwrap(),
        connector_id: ConnectorId::new("test-connector").unwrap(),
        connector_version: "0.1.0".to_string(),
        hardware_identity: format!("test-connector:{id}"),
        family: "test-family".to_string(),
        board_identity: Some("test-board-rev-a".to_string()),
        transport_reference: Some("COM3".to_string()),
        runtime_id: RuntimeId::new("runtime-1").unwrap(),
        support_tier: SupportTier::Certified,
        lifecycle_state: DeviceLifecycleState::Discovered,
        capabilities_json: Some(r#"["identify","read_health"]"#.to_string()),
        firmware_artifact_ref: None,
        config_artifact_ref: None,
        model_artifact_ref: None,
        health_json: None,
        last_seen_at: T0.to_string(),
        created_at: T0.to_string(),
        updated_at: T0.to_string(),
    }
}

pub fn sample_job(id: &str, idempotency_key: &str) -> NewJob {
    NewJob {
        job_id: JobId::new(id).unwrap(),
        state: JobState::Queued,
        idempotency_key: idempotency_key.to_string(),
        correlation_id: CorrelationId::new("corr-1").unwrap(),
        created_at: T0.to_string(),
        updated_at: T0.to_string(),
    }
}
