//! Test-only helper binary (`test-support` feature; never compiled into a
//! production `cargo build --release -p loki-credential-native`).
//!
//! `P1-NODE-IPC-CLIENT-001` §6 Deliverable 6's cross-language round-trip
//! test needs a real Node test process to drive **both** the real Rust
//! minting call and the real compiled Node native addon in one test run.
//! This binary is the mechanism that lets a Node/vitest test do the former
//! without an FFI call from JS straight into `loki-storage` (which would
//! duplicate/bypass the addon boundary under test) and without ever putting
//! plaintext on this process's own argv/env/stdout:
//!
//! ```text
//! mint_test_credential <desktop_installation|agent_connection> <protected-path> <plaintext-evidence-path>
//! ```
//!
//! Mints (or loads, idempotently) the named credential kind at
//! `<protected-path>` via the real `loki_storage::credential::
//! mint_or_load_credential`, then writes the resulting plaintext bytes
//! **raw** to `<plaintext-evidence-path>` — a transient, test-local file the
//! calling Node test reads exactly once and deletes immediately afterward
//! (never a committed fixture, never printed/logged). Both path arguments
//! are plain filesystem paths, never secret material, so this satisfies the
//! same "no secret bytes on argv/env/stdout" posture the shipped addon
//! itself follows, even though this binary is test-only scaffolding, not
//! part of the addon's own boundary.

use std::env;
use std::process::ExitCode;

use loki_storage::credential::{self, CredentialKind};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let [_, kind_arg, protected_path, evidence_path] = &args[..] else {
        eprintln!(
            "usage: mint_test_credential <desktop_installation|agent_connection> <protected-path> <plaintext-evidence-path>"
        );
        return ExitCode::FAILURE;
    };

    let kind = match kind_arg.as_str() {
        "desktop_installation" => CredentialKind::DesktopInstallation,
        "agent_connection" => CredentialKind::AgentConnection,
        other => {
            eprintln!("unknown credential kind: {other}");
            return ExitCode::FAILURE;
        }
    };

    match credential::mint_or_load_credential(kind, protected_path.as_ref()) {
        Ok(secret) => match std::fs::write(evidence_path, &secret) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("failed to write plaintext evidence file: {err}");
                ExitCode::FAILURE
            }
        },
        Err(err) => {
            eprintln!("failed to mint/load {kind} credential: {err}");
            ExitCode::FAILURE
        }
    }
}
