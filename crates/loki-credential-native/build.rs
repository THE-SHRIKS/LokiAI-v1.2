//! `P1-NODE-IPC-CLIENT-001`: standard napi-rs build glue — emits the
//! `cargo:` directives needed to link this cdylib against Node's/Electron's
//! delay-loaded N-API symbol table instead of a specific `node.exe`/
//! `electron.exe` at build time.

fn main() {
    napi_build::setup();
}
