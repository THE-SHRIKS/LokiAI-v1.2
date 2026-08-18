//! Desktop discovery: how the Electron main process (a later node,
//! `DSK-002`+ — not built here, per `Docs/graph/tasks/RUN-003.md`
//! Deliverable 3) finds and verifies a running background host, without
//! inventing an undocumented ad hoc protocol.
//!
//! ## The mechanism
//!
//! Two parts, both anchored under the per-user
//! `%LOCALAPPDATA%\LokiAI\runtime` directory ([`super::paths::HostPaths`]):
//!
//! 1. **A well-known, protocol-versioned, per-user named pipe**
//!    ([`well_known_pipe_name`]) —
//!    `\\.\pipe\LokiAI-runtime-v{major}-{user-sid}`. Per-user (embedding the
//!    resolved Windows SID, not just the process's `%LOCALAPPDATA%`) because
//!    Windows named pipes are a machine-wide namespace, not scoped to a
//!    login session on their own — this matches `Docs/ARCHITECTURE.md` §5's
//!    "installation/user-specific name" requirement and lets more than one
//!    interactively-logged-on user run their own host without colliding. A
//!    client that already knows this convention does not need to read any
//!    file to know *where* to connect.
//! 2. **A small JSON discovery file** (`host.json`, written by
//!    `src/host/compose.rs`) recording what the host itself last observed
//!    about its own lifecycle: `state` (`"starting"`, `"serving"`, or
//!    `"stopped"`), `pid`, `pipe_name`, `started_at_unix_millis`, and
//!    `updated_at_unix_millis`. This file exists so a client can cheaply
//!    distinguish "definitely not running" (file absent, or explicitly
//!    marked `"stopped"` by a clean shutdown) from "something claims to be
//!    running" *before* attempting a real connection.
//!
//! ## Why [`HostStatus::Healthy`] is never taken on the file's word alone
//!
//! Per `AGENTS.md` rule 1 ("no state silently defaults to healthy"),
//! [`locate`] only ever reports [`HostStatus::Healthy`] after a real,
//! bounded handshake + `ping` round-trip over the actual pipe — never from
//! file content alone. The file only disambiguates *why* a live attempt
//! failed: never started, crashed while claiming to be running, or still
//! starting up.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::host::clock::unix_millis_now;
use crate::host::paths::HostPaths;

/// How long a `"starting"` discovery file is given the benefit of the doubt
/// before a failed live connect is reported as [`HostStatus::Unreachable`]
/// instead of [`HostStatus::Starting`].
const STARTING_GRACE: Duration = Duration::from_secs(30);

/// The lifecycle state the host itself last wrote to the discovery file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedState {
    Starting,
    Serving,
    Stopped,
}

impl RecordedState {
    fn as_str(self) -> &'static str {
        match self {
            RecordedState::Starting => "starting",
            RecordedState::Serving => "serving",
            RecordedState::Stopped => "stopped",
        }
    }
}

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryFileV1 {
    schema_version: u32,
    state: String,
    pipe_name: String,
    pid: u32,
    started_at_unix_millis: u64,
    updated_at_unix_millis: u64,
}

/// Writes the discovery file at `path`, overwriting any previous content.
/// `started_at_unix_millis` should be the same value across the
/// `Starting` -> `Serving` transition for one host run (it identifies when
/// *this instance* began starting, not when the file was last touched —
/// that is `updated_at_unix_millis`, always set to "now").
pub fn write(
    path: &Path,
    state: RecordedState,
    pipe_name: &str,
    started_at_unix_millis: u64,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let record = DiscoveryFileV1 {
        schema_version: SCHEMA_VERSION,
        state: state.as_str().to_string(),
        pipe_name: pipe_name.to_string(),
        pid: std::process::id(),
        started_at_unix_millis,
        updated_at_unix_millis: unix_millis_now(),
    };
    let json = serde_json::to_vec_pretty(&record)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    std::fs::write(path, json)
}

fn read(path: &Path) -> std::io::Result<Option<DiscoveryFileV1>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let record: DiscoveryFileV1 = serde_json::from_slice(&bytes)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    Ok(Some(record))
}

fn recently_updated(record: &DiscoveryFileV1) -> bool {
    let updated = Duration::from_millis(record.updated_at_unix_millis);
    let now = Duration::from_millis(unix_millis_now());
    now.saturating_sub(updated) <= STARTING_GRACE
}

/// The well-known, deterministic, protocol-versioned, per-current-user
/// named-pipe address — see this module's top doc comment.
pub fn well_known_pipe_name() -> std::io::Result<String> {
    let sid = loki_ipc::acl::current_user_sid_string()?;
    Ok(format!(
        r"\\.\pipe\LokiAI-runtime-v{}-{sid}",
        loki_ipc::PROTOCOL_VERSION.major
    ))
}

/// Truthful lifecycle states a discovery caller (the future Electron main
/// process integration) can observe. Distinct on purpose — see
/// `Docs/graph/tasks/RUN-003.md` Deliverable 4: "no state silently defaults
/// to healthy."
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostStatus {
    /// No discovery file, or the file was explicitly marked `"stopped"` by
    /// a clean shutdown.
    NotRunning,
    /// A discovery file recently claimed the host is starting, and a live
    /// connect has not yet succeeded (still within its startup grace
    /// window).
    Starting,
    /// A real handshake and `ping` round-trip succeeded just now.
    Healthy,
    /// A discovery file claims (or recently claimed) the host is/was
    /// serving, but a live connect/ping attempt did not succeed — e.g. an
    /// unclean crash left a stale file behind, or the file is missing but
    /// unreadable/corrupt.
    Unreachable { detail: String },
}

/// Locates a running host: reads the discovery file (if any) at
/// `paths.discovery_file()`, then attempts a real, bounded handshake+`ping`
/// against `pipe_name` using `token`. Never hangs past
/// `connect_timeout`/`call_timeout`, and never reports [`HostStatus::Healthy`]
/// without a real successful round-trip (see this module's top doc
/// comment).
pub async fn locate(
    paths: &HostPaths,
    pipe_name: &str,
    token: &loki_ipc::auth::AuthToken,
    connect_timeout: Duration,
    call_timeout: Duration,
) -> HostStatus {
    let record = match read(&paths.discovery_file()) {
        Ok(None) => return HostStatus::NotRunning,
        Ok(Some(record)) => Some(record),
        Err(err) => {
            tracing::debug!(
                error = %err,
                "discovery file present but unreadable/malformed; attempting a live probe anyway"
            );
            None
        }
    };

    if let Some(record) = &record
        && record.state == RecordedState::Stopped.as_str()
    {
        return HostStatus::NotRunning;
    }

    match try_ping(pipe_name, token, connect_timeout, call_timeout).await {
        Ok(()) => HostStatus::Healthy,
        Err(detail) => {
            let starting_and_fresh = record.as_ref().is_some_and(|r| {
                r.state == RecordedState::Starting.as_str() && recently_updated(r)
            });
            if starting_and_fresh {
                HostStatus::Starting
            } else {
                HostStatus::Unreachable { detail }
            }
        }
    }
}

async fn try_ping(
    pipe_name: &str,
    token: &loki_ipc::auth::AuthToken,
    connect_timeout: Duration,
    call_timeout: Duration,
) -> Result<(), String> {
    let mut config =
        loki_ipc::ClientConfig::new(pipe_name, token.clone(), "loki-runtime-discovery");
    config.connect_timeout = connect_timeout;
    let client = loki_ipc::Client::connect(config)
        .await
        .map_err(|err| err.to_string())?;
    client
        .ping("discovery-probe", call_timeout)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_token() -> loki_ipc::auth::AuthToken {
        loki_ipc::auth::AuthToken::new("test-token-value-long-enough-to-be-plausible")
    }

    /// Real positive case for `Docs/graph/tasks/RUN-003.md` Deliverable 5:
    /// "discovery mechanism actually locates a running host" — binds a real
    /// `loki_ipc::Server` on a real named pipe, writes a matching `"serving"`
    /// discovery file, and asserts [`locate`] only reports
    /// [`HostStatus::Healthy`] after a real handshake+`ping` round trip
    /// (never from the file's claim alone — see this module's top doc
    /// comment).
    #[tokio::test]
    async fn a_real_running_host_is_located_and_reported_healthy() {
        let dir = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(dir.path());
        let pipe_name = format!(
            r"\\.\pipe\loki-runtime-discovery-test-{}",
            uuid::Uuid::new_v4()
        );
        let token = test_token();

        let server_config = loki_ipc::ServerConfig::new(pipe_name.clone(), token.clone());
        let mut server = loki_ipc::Server::bind(server_config).expect("server must bind");
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let serve_task = tokio::spawn(async move {
            let shutdown: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
                Box::pin(async move {
                    let _ = stop_rx.await;
                });
            server.serve(shutdown).await;
        });

        write(
            &paths.discovery_file(),
            RecordedState::Serving,
            &pipe_name,
            unix_millis_now(),
        )
        .unwrap();

        let status = tokio::time::timeout(
            Duration::from_secs(3),
            locate(
                &paths,
                &pipe_name,
                &token,
                Duration::from_secs(2),
                Duration::from_secs(2),
            ),
        )
        .await
        .expect("locate must not hang against a real running host");
        assert_eq!(status, HostStatus::Healthy);

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(2), serve_task).await;
    }

    #[tokio::test]
    async fn no_discovery_file_reports_not_running_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(dir.path());
        let status = tokio::time::timeout(
            Duration::from_millis(500),
            locate(
                &paths,
                r"\\.\pipe\LokiAI-runtime-test-nonexistent",
                &test_token(),
                Duration::from_millis(200),
                Duration::from_millis(200),
            ),
        )
        .await
        .expect("locate must not hang when no discovery file exists");
        assert_eq!(status, HostStatus::NotRunning);
    }

    #[tokio::test]
    async fn a_file_explicitly_marked_stopped_reports_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(dir.path());
        write(
            &paths.discovery_file(),
            RecordedState::Stopped,
            "irrelevant",
            0,
        )
        .unwrap();

        let status = tokio::time::timeout(
            Duration::from_millis(500),
            locate(
                &paths,
                r"\\.\pipe\LokiAI-runtime-test-stopped",
                &test_token(),
                Duration::from_millis(200),
                Duration::from_millis(200),
            ),
        )
        .await
        .expect("locate must not hang");
        assert_eq!(status, HostStatus::NotRunning);
    }

    #[tokio::test]
    async fn a_stale_starting_file_with_no_real_listener_reports_unreachable() {
        let dir = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(dir.path());
        // Written directly (bypassing `write()`'s "now" timestamp) so the
        // record is definitely older than STARTING_GRACE.
        let stale = DiscoveryFileV1 {
            schema_version: SCHEMA_VERSION,
            state: RecordedState::Starting.as_str().to_string(),
            pipe_name: "irrelevant".to_string(),
            pid: std::process::id(),
            started_at_unix_millis: 0,
            updated_at_unix_millis: 0,
        };
        std::fs::write(paths.discovery_file(), serde_json::to_vec(&stale).unwrap()).unwrap();

        let status = tokio::time::timeout(
            Duration::from_secs(3),
            locate(
                &paths,
                r"\\.\pipe\LokiAI-runtime-test-stale",
                &test_token(),
                Duration::from_millis(300),
                Duration::from_millis(300),
            ),
        )
        .await
        .expect("locate must not hang against an unreachable pipe");
        assert!(matches!(status, HostStatus::Unreachable { .. }));
    }

    #[tokio::test]
    async fn a_fresh_starting_file_with_no_real_listener_yet_reports_starting() {
        let dir = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(dir.path());
        write(
            &paths.discovery_file(),
            RecordedState::Starting,
            "irrelevant",
            unix_millis_now(),
        )
        .unwrap();

        let status = tokio::time::timeout(
            Duration::from_secs(3),
            locate(
                &paths,
                r"\\.\pipe\LokiAI-runtime-test-starting",
                &test_token(),
                Duration::from_millis(300),
                Duration::from_millis(300),
            ),
        )
        .await
        .expect("locate must not hang");
        assert_eq!(status, HostStatus::Starting);
    }

    #[test]
    fn well_known_pipe_name_is_deterministic_and_versioned() {
        let a = well_known_pipe_name().unwrap();
        let b = well_known_pipe_name().unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with(r"\\.\pipe\LokiAI-runtime-v1-S-1-"));
    }

    #[test]
    fn write_then_read_round_trips_every_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.json");
        write(&path, RecordedState::Serving, r"\\.\pipe\test", 12_345).unwrap();
        let record = read(&path).unwrap().expect("file should be present");
        assert_eq!(record.schema_version, SCHEMA_VERSION);
        assert_eq!(record.state, "serving");
        assert_eq!(record.pipe_name, r"\\.\pipe\test");
        assert_eq!(record.started_at_unix_millis, 12_345);
        assert_eq!(record.pid, std::process::id());
        assert!(record.updated_at_unix_millis > 0);
    }

    #[test]
    fn read_of_a_missing_file_returns_none_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = read(&dir.path().join("does-not-exist.json"));
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn read_of_a_malformed_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.json");
        std::fs::write(&path, b"this is not json").unwrap();
        assert!(read(&path).is_err());
    }
}
