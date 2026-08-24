# DAT-002 verification evidence

Captured: 2026-08-24T09:25:43Z  
Revision: `7ef9bc3f38b7666b01ea8f89eeaa81caa947a6ab`  
Host: Windows 11 x64, `x86_64-pc-windows-msvc`  
Toolchain: Rust/Cargo 1.97.1, pnpm 11.19.0, Strawberry Perl 5.42.2.1 for vendored OpenSSL

## Required command gates

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, exit 0 |
| `cargo build --workspace` | pass, exit 0 |
| `cargo test --workspace` | pass, exit 0 |
| `pnpm run verify` | pass, exit 0; includes Prettier, ESLint, TS typechecks/tests, Electron tests, Rust format/build/tests |
| `git diff --check` | pass, no whitespace errors |

The native debug build emits MSVC `LNK4099` warnings because vendored OpenSSL
objects refer to an `ossl_static.pdb` that the upstream vendored build does not
place beside the archive. Linking succeeds and Clippy with warnings denied
passes. This is a debug-symbol quality warning, not an encryption/test bypass;
it remains named for packaging review.

The aggregate ESLint gate retains four pre-existing renderer warnings (no
errors) in files outside DAT-002 ownership. The command exits zero.

## Data/security scenarios

| Scenario | Result/evidence |
|---|---|
| Plaintext core DB migration | pass: `plaintext_migration_preserves_rows_and_removes_plaintext_artifacts` |
| Missing/wrong key | pass: explicit failure, exact database bytes unchanged |
| DPAPI blob confidentiality | pass: protected blob byte scan excludes known plaintext and unprotect restores it |
| Key rotation | pass: protected blob changes, row preserved, file remains opaque |
| Interrupted migration before swap | pass: original restored, migration retried, marker/rollback cleaned |
| Interrupted key promotion | pass: pending key verified before promotion and cleanup |
| Managed encrypted backup | pass: integrity-checked, byte-scan opaque, readable with protected source key |
| Monitoring/rules file | pass: real plaintext migration, telemetry marker insert, unkeyed open denied, byte scan opaque |
| Incidents/evidence file | pass: real plaintext migration and work-intent marker, unkeyed open denied, byte scan opaque |
| Runtime IPC token | pass: DPAPI create/reuse, legacy plaintext migration/removal, real child-process IPC test |
| Core corruption/restart/transactions | pass as part of full workspace suite |

All file-backed tests use temporary real Windows files and the production
SQLCipher/DPAPI code. No mock database, fake encryption, cloud behavior, or
hardware operation is used.

## Dependency/licence evidence

`cargo metadata --locked` and `cargo tree -p loki-storage` resolve:

- `rusqlite 0.40.2` (MIT);
- `libsqlite3-sys 0.38.2` (MIT wrapper) with its bundled
  `sqlcipher/LICENSE` carrying the Zetetic BSD-style notice;
- `openssl-sys 0.9.117` (MIT) and `openssl-src 300.6.1+3.6.3`
  (MIT/Apache-2.0 Rust packaging; upstream OpenSSL notice applies);
- `windows-sys 0.61.2` (MIT OR Apache-2.0).

`cargo-audit` and `cargo-deny` are not installed on this host, so no claim is
made that their current advisory databases were checked. The workspace pnpm
lockfile supply-chain policy gate passed. Exact native notice/SBOM requirements
are recorded in `Docs/TECH_STACK.md` and `Docs/RELEASE.md`.

## Mandatory verification not available

- Cross-Windows-identity DPAPI denial: blocked. `Get-LocalUser` shows only
  `lauki` enabled; built-in alternative accounts are disabled. No separate
  service test identity exists. No installed/running VMware tooling was found.
  Creating a local account or changing service identity is external OS state
  and was not performed without exact authorization.
- Independent data-security reviewer: not performed. This implementation and
  the adversarial review were completed by the same accountable worker, so it
  cannot honestly satisfy the packet's independent-review criterion.

Per the accepted task packet's stop conditions, DAT-002 therefore remains
`BLOCKED_DEPENDENCY`, not `COMPLETE`, despite all runnable implementation gates
passing.
