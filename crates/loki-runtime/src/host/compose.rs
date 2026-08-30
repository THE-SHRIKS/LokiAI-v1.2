//! Composes the background host's real subsystems — `RUN-001`'s health/
//! logging, `RUN-002`'s IPC listener, and `DAT-001`'s storage — into one
//! runnable unit, and drives the desktop-discovery file's lifecycle
//! (`starting` -> `serving` -> `stopped`) alongside it.
//!
//! `ComposedHost::open` does the synchronous/fallible startup work (bind the
//! database, migrate it, load/create the local IPC token, bind the named
//! pipe) so a failure to start is reported clearly and immediately, rather
//! than surfacing later as a mysteriously-unhealthy running process.
//! `ComposedHost::run` then drives the long-lived service loop, reusing
//! `crate::runtime::run` (`RUN-001`, unmodified) for the existing shutdown-
//! vs-heartbeat race, with the IPC accept loop running alongside it and
//! stopped afterward.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use loki_ipc::auth::AuthToken;
use loki_ipc::{Server, ServerConfig};
use loki_storage::{SecureMemoryError, SecureMemoryRuntime, Storage};

use crate::host::clock::{now_utc_iso8601, unix_millis_now};
use crate::host::discovery::{self, RecordedState};
use crate::host::paths::HostPaths;
use crate::host::token;
use crate::runtime::{self, RunOutcome};
use crate::services::RuntimeServices;
use crate::shutdown::ShutdownReason;

/// How long [`ComposedHost::run`] waits for the IPC accept loop to drain
/// after the primary shutdown/heartbeat race ends, before giving up —
/// matches `crates/loki-ipc/src/server.rs`'s own bounded-shutdown-grace
/// philosophy (`WRITER_SHUTDOWN_GRACE`) rather than trusting an unbounded
/// drop to finish promptly.
const IPC_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Resolved configuration for one background host instance.
#[derive(Debug, Clone)]
pub struct HostConfig {
    pub paths: HostPaths,
    pub pipe_name: String,
}

impl HostConfig {
    /// Production configuration: per-user `%LOCALAPPDATA%` paths and the
    /// well-known, protocol-versioned, per-user pipe name.
    pub fn for_current_user() -> std::io::Result<Self> {
        let paths = HostPaths::default_for_current_user()?;
        let pipe_name = discovery::well_known_pipe_name()?;
        Ok(Self { paths, pipe_name })
    }
}

/// Why [`ComposedHost::open`] failed to start.
#[derive(Debug)]
pub enum HostStartError {
    Io(std::io::Error),
    SecureMemory(SecureMemoryError),
    Storage(loki_storage::StorageError),
}

impl std::fmt::Display for HostStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostStartError::Io(err) => write!(f, "i/o error starting background host: {err}"),
            HostStartError::SecureMemory(err) => {
                write!(f, "secure-memory error starting background host: {err}")
            }
            HostStartError::Storage(err) => {
                write!(f, "storage error starting background host: {err}")
            }
        }
    }
}

impl std::error::Error for HostStartError {}

impl From<std::io::Error> for HostStartError {
    fn from(err: std::io::Error) -> Self {
        HostStartError::Io(err)
    }
}

impl From<loki_storage::StorageError> for HostStartError {
    fn from(err: loki_storage::StorageError) -> Self {
        HostStartError::Storage(err)
    }
}

impl From<SecureMemoryError> for HostStartError {
    fn from(err: SecureMemoryError) -> Self {
        HostStartError::SecureMemory(err)
    }
}

/// A fully started background host: storage migrated, IPC pipe bound, and
/// the desktop-discovery file already reflecting `"serving"`.
pub struct ComposedHost {
    storage: Arc<Storage>,
    /// Application services composed over the same migrated store the host
    /// just opened, so no adapter or future business method can open a
    /// second, divergent database. See `crate::services` for the real,
    /// atomic mutation-submission boundary this composes.
    services: RuntimeServices,
    server: Server,
    discovery_file: PathBuf,
    started_at_unix_millis: u64,
    secure_memory: SecureMemoryRuntime,
}

impl ComposedHost {
    /// Starts everything synchronously: creates the base directory,
    /// opens/migrates the `DAT-001` database, writes the discovery file as
    /// `"starting"`, loads or creates the local IPC token, binds the named
    /// pipe, then updates the discovery file to `"serving"`. Any failure at
    /// any step is returned immediately — this never leaves a
    /// half-initialized host silently running.
    pub fn open(config: &HostConfig) -> Result<Self, HostStartError> {
        // This is deliberately the first fallible startup action.  A missing,
        // invalid, unlocked, or unattested arena therefore touches no base
        // directory, database, key sidecar, token, discovery file, or pipe.
        let secure_memory = SecureMemoryRuntime::bootstrap()?;
        Self::open_with_arena(config, secure_memory)
    }

    /// Test-only alternate entrypoint: identical to [`Self::open`] in every
    /// respect except how `secure_memory` is obtained. Production `open()`
    /// always calls the real [`SecureMemoryRuntime::bootstrap`], which goes
    /// through `loki_storage::secure_profile::accepted_profile()` and fails
    /// closed (`Profile(Unavailable)`) at this revision because no digest-
    /// bound profile is embedded — by design, per ADR-0005/`DAT-002-MEM-CODE-001`.
    /// That production gate is completely untouched here: this function
    /// instead bootstraps `loki_storage::secure_memory::test_support`'s
    /// disposable, non-production arena (the same bypass mechanism the
    /// crate's own tests and 9 other consumer crates already use via
    /// `require_ready()`), owned exactly the way `ComposedHost` needs so it
    /// can shut the arena down itself later, matching `open()`'s own
    /// lifecycle exactly.
    ///
    /// Compiled only for this crate's own tests (`cfg(test)`) or the opt-in
    /// `test-support` Cargo feature — off by default, only turned on by this
    /// crate's own self-referential `[dev-dependencies]` entry — so this
    /// never reaches a release build and `open()` (the sole entrypoint
    /// `crates/loki-runtime/src/main.rs`/real startup calls) has zero
    /// reachability into this path.
    #[cfg(any(test, feature = "test-support"))]
    pub fn open_with_test_arena(config: &HostConfig) -> Result<Self, HostStartError> {
        let secure_memory =
            loki_storage::secure_memory::test_support::bootstrap_disposable_test_arena();
        Self::open_with_arena(config, secure_memory)
    }

    /// Shared startup body once a `secure_memory` arena (production-bootstrapped
    /// or, in test builds only, disposable-test-bootstrapped) is already in
    /// hand. See [`Self::open`]/[`Self::open_with_test_arena`].
    fn open_with_arena(
        config: &HostConfig,
        secure_memory: SecureMemoryRuntime,
    ) -> Result<Self, HostStartError> {
        config.paths.ensure_base_dir()?;

        let database_file = config.paths.database_file();
        let storage = Storage::open_managed(&database_file, &config.paths.database_key_file())?;
        let report = storage.run_migrations(&now_utc_iso8601())?;
        tracing::info!(applied = ?report.applied_versions, "background host: storage migrated");
        let storage = Arc::new(storage);
        let services = RuntimeServices::compose(Arc::clone(&storage));

        let started_at_unix_millis = unix_millis_now();
        let discovery_file = config.paths.discovery_file();
        discovery::write(
            &discovery_file,
            RecordedState::Starting,
            &config.pipe_name,
            started_at_unix_millis,
        )?;

        let raw_token = token::load_or_create_with_legacy(
            &config.paths.protected_token_file(),
            &config.paths.token_file(),
        )?;
        let server_config = ServerConfig::new(config.pipe_name.clone(), AuthToken::new(raw_token));
        let server = Server::bind(server_config)?;

        discovery::write(
            &discovery_file,
            RecordedState::Serving,
            &config.pipe_name,
            started_at_unix_millis,
        )?;
        tracing::info!(pipe_name = %config.pipe_name, "background host: ipc listener bound and serving");

        Ok(Self {
            storage,
            services,
            server,
            discovery_file,
            started_at_unix_millis,
            secure_memory,
        })
    }

    /// The opened, migrated storage handle. Exposed so a caller (this
    /// crate's own tests, and future consumers such as `JOB-001`) can use it
    /// directly — no business method reads/writes it over IPC yet (`RUN-002`
    /// ships only `ping`/`echo_stream`; real business methods are later
    /// nodes' scope).
    pub fn storage(&self) -> &Storage {
        self.storage.as_ref()
    }

    /// The runtime's composed application services, bounded to the read
    /// projections and closed mutation port in `crate::services`. Future
    /// authenticated IPC business methods read through this instead of
    /// opening or querying storage directly.
    pub fn services(&self) -> &RuntimeServices {
        &self.services
    }

    /// Runs the host until `shutdown_signal` resolves (clean exit) or
    /// `heartbeat` ends on its own (treated as a crash) — reusing
    /// `crate::runtime::run` (`RUN-001`, unmodified) for that race exactly
    /// as before. The IPC accept loop runs concurrently and is told to stop
    /// once that race ends, bounded by [`IPC_SHUTDOWN_GRACE`]. On return,
    /// the discovery file is updated to `"stopped"` best-effort, so a
    /// subsequent [`crate::host::discovery::locate`] call reports
    /// [`crate::host::discovery::HostStatus::NotRunning`] immediately
    /// without needing a live connect attempt.
    pub async fn run<S, H>(self, shutdown_signal: S, heartbeat: H) -> RunOutcome
    where
        S: Future<Output = ShutdownReason>,
        H: Future<Output = ()> + Send + 'static,
    {
        let ComposedHost {
            storage,
            services,
            mut server,
            discovery_file,
            started_at_unix_millis,
            secure_memory,
        } = self;

        let (ipc_stop_tx, ipc_stop_rx) = tokio::sync::oneshot::channel::<()>();
        let ipc_shutdown: std::pin::Pin<Box<dyn Future<Output = ()> + Send>> =
            Box::pin(async move {
                let _ = ipc_stop_rx.await;
            });
        let ipc_task = tokio::spawn(async move {
            server.serve(ipc_shutdown).await;
        });

        let outcome = runtime::run(shutdown_signal, heartbeat).await;

        let _ = ipc_stop_tx.send(());
        if tokio::time::timeout(IPC_SHUTDOWN_GRACE, ipc_task)
            .await
            .is_err()
        {
            tracing::warn!("ipc accept loop did not stop within the shutdown grace period");
        }

        if let Err(err) = discovery::write(
            &discovery_file,
            RecordedState::Stopped,
            "",
            started_at_unix_millis,
        ) {
            tracing::warn!(error = %err, "failed to mark the discovery file stopped on shutdown");
        }

        // No SQLite connection may survive arena teardown.  `storage` is an
        // `Arc<Storage>` shared with every service in `services` (see
        // `crate::services::RuntimeServices::compose`), so every other clone
        // must drop first or the final `drop(storage)` below would merely
        // decrement a refcount instead of closing the last core connection
        // before `sqlite3_shutdown`, wipe/unlock/free, and current-process
        // working-set restoration.
        drop(services);
        drop(storage);
        if let Err(err) = secure_memory.shutdown() {
            tracing::error!(error = %err, "secure-memory shutdown did not complete cleanly");
        }

        outcome
    }
}
