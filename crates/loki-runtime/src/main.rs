//! Process entry point.
//!
//! Deliberately thin: parse `argv` and forward to the `loki_runtime` library
//! (see `lib.rs`), which is where every testable/behavioral decision lives.
//! Keeping `main` synchronous (no `#[tokio::main]`) means a malformed CLI
//! argument is rejected and printed before any async runtime is ever built —
//! there is no path where a bad argument can panic inside Tokio.

use std::process::ExitCode;

fn main() -> ExitCode {
    loki_runtime::run(std::env::args().collect())
}
