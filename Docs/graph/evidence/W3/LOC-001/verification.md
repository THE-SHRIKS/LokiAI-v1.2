# LOC-001 verification — 2026-08-24

- Implementation revision: `9d29f56`
- Host: Windows 11, current `lauki` interactive identity
- Worker: Codex
- Device/cloud effects: none

## Results

| Gate | Result |
| --- | --- |
| `cargo test -p loki-local-authority` | pass; 7/7 tests |
| `cargo clippy -p loki-local-authority --all-targets -- -D warnings` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `pnpm run verify` outside the restricted process sandbox | pass; TypeScript, Electron, Cargo build, and full Cargo workspace tests |

The first sandboxed `pnpm run verify` attempt reached TypeScript typechecking
and failed with Windows `spawn EPERM`. It was rerun unchanged outside the
process sandbox and passed. This is an environment restriction, not a product
failure. Existing renderer ESLint output contains four warnings and zero errors;
the warnings predate LOC-001 and are outside this node's ownership.

## Security scenarios

- current Windows SID is resolved through the accepted RUN-002 Win32 token path;
- a different SID cannot adopt the installation or issue a session;
- a session cannot move to another authenticated IPC session;
- expiry and revocation fail closed;
- concurrent revocation has exactly one successful transition;
- restart preserves installation and revocation metadata;
- malformed SID/session/lifetime inputs are rejected;
- every cloud capability returns `CloudUnavailable`;
- no OAuth token, bearer credential, password, account, workspace, role, or
  authority secret is persisted.
