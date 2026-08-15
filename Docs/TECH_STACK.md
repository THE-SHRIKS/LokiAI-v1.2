# Technology Stack Baseline — LokiAI Main v1.2

Status: **Normative proposed baseline; exact pins accepted in FND-002**  
Reviewed: **2026-08-15**

## 1. Verdict

The selected architecture is suitable for a production Windows hardware-control
product in 2026:

- Electron preserves the accepted React desktop interface and provides native
  lifecycle, browser-auth callback, tray, notification, and update integration.
- Rust is the correct authority boundary for hardware access, durable jobs,
  monitoring, connector execution, isolation control, and local persistence.
- A standalone TypeScript/Node MCP server uses a Tier-1 official MCP SDK while
  keeping provider-specific setup out of the domain.
- SQLite with full-database encryption is appropriate for local durable state.
- WiX can install the desktop, runtime, MCP host, and service/background
  components as one signed Windows product.

This is not approval to place device authority in Electron or to execute build
scripts in the runtime process. The process boundaries in `ARCHITECTURE.md` and
`BUILD_EXECUTION.md` remain mandatory.

## 2. Supported product platform

| Item | v1.2 baseline |
|---|---|
| Operating system | Windows 11 x64 on Microsoft-supported editions/builds |
| CPU architecture | x64; arm64 is a future qualification target |
| Desktop distribution | Direct, signed installer baseline; Store packaging remains an OQ-004 decision |
| Device transports | Connector-declared local transports; USB/serial paths are never exposed to the renderer or MCP client |
| Always-on operation | `gateway-service` on an always-on host; a powered-off developer PC cannot monitor or repair a device |

Windows 10 and 32-bit Windows are not v1.2 production targets. Adding an OS or
architecture requires packaging, driver, connector, security, and hardware
qualification evidence—not only a successful compile.

## 3. 2026 implementation baseline

Versions below are the reviewed baseline on 2026-08-15. `FND-002` shall resolve
and lock exact patches, record checksums where applicable, and rerun the full
compatibility gate. A prerelease is never selected for production merely because
it is newer.

| Layer | Selected baseline | Reason and constraints |
|---|---|---|
| Electron | Latest supported Electron 43 patch; 43.4.0 was current at review | Current stable line; Electron 44 was still beta. Keep Chromium/Node security patches current and plan upgrades before EOL. |
| Renderer | React 19.2.x; 19.2.7 was current | Preserves the existing UI and moves it to the current React 19 line. |
| Bundler | Vite 8.2.x with compatible `@vitejs/plugin-react` | Vite 8.2 was the regularly patched line. Build Electron main, preload, and renderer explicitly; do not depend on Electron Forge's experimental Vite plugin. |
| Type system | TypeScript 6.0.x, strict project references | Current transition release. The migration must remove deprecated options and pass before it becomes the lockfile baseline. |
| Node toolchain | Node 24 LTS; 24.19.0 was current LTS | Used for workspace tooling and the standalone MCP distribution. Do not require an end user to install Node globally. |
| Package manager | One exact Corepack-managed pnpm stable release | Efficient monorepo workspaces and one lockfile. Enable delayed dependency adoption and explicit build-script allowlisting. |
| Styling and motion | Tailwind CSS 4 and the existing Motion/Lucide stack | Preserves Chamber & Olive. Patch versions are pinned after compatibility and license checks. |
| MCP | Official TypeScript SDK supporting MCP `2026-07-28` and the one previous revision required by certified clients | TypeScript is Tier 1 for the current protocol; the Rust SDK was beta at review. Provider feature support can lag the protocol, so compatibility is tested, bounded, and edge-only. MCP ships with its own Node 24 runtime or equivalent self-contained launcher, not Electron's embedded Node. |
| Hardware runtime | Rust 1.97.1 stable, edition 2024 | Memory-safe native boundary, strong concurrency/state-machine support, and Windows API access. `rust-toolchain.toml` pins the release and components. |
| Async/diagnostics | Compatible stable Tokio, `tracing`, structured JSON logs | Exact crates are selected through dependency review; no runtime framework owns domain state transitions. |
| Local data | SQLCipher 4.16.x-compatible SQLite, runtime-owned; Community is the default candidate pending OQ-007 | Full database encryption with the key protected by Windows DPAPI/credential storage. Edition/support, license attribution, WAL behavior, backup, migration, and key-recovery tests are release gates. |
| Contracts | Canonical JSON Schema 2020-12 plus generated TypeScript/Rust types | Runtime validation at every boundary and deterministic drift checks. Generated types do not replace schema validation. |
| Desktop testing | Vitest-compatible unit runner and Playwright desktop E2E | Browser preview is not release proof; E2E drives a packaged Electron build. |
| Rust testing | Built-in tests plus reviewed property/nextest tooling where useful | State-machine, parser, crash, race, and containment tests are first-class. |
| Packaging | Electron Forge core/packager and fuses; WiX Toolset Burn bundle | Forge is Electron's recommended packaging path. WiX provides a signed product bundle capable of installing desktop plus runtime/service/MCP components. |
| Updates | LokiAI-signed component manifest and payloads with staged rollout | Update is atomic across compatible component versions. HTTPS alone is insufficient. |

## 4. Existing renderer drift

`LokiAI Desktop UI/package.json` is an accepted visual prototype, not the locked
production dependency set. The foundation/migration nodes must address:

- Vite 6.2 and TypeScript 5.8 are behind the selected supported lines;
- React 19.0 should move to the current 19.2 patch line;
- the package mixes runtime and development dependencies and duplicates Vite;
- `@google/genai`, Express, dotenv, and their types are not imported by the
  renderer and appear to be generator-host leftovers; remove them unless a task
  proves a production need;
- `.env.example` contains Gemini/AI Studio and hosted-web assumptions that do not
  belong in the privileged desktop product;
- the `clean` script uses a Unix command and is not a valid Windows-first root
  command;
- TypeScript is not currently strict and permits JavaScript;
- auth/onboarding and product state still use browser storage, fixtures, and
  timer-driven success behavior.

`FND-002` establishes the toolchains and `UI-001` replaces presentation-only
behavior. Dependency upgrades must not be mixed with a visual redesign.

## 5. Dependency and version policy

1. Production manifests and toolchains use exact versions; lockfiles are
   committed and immutable in CI.
2. Each addition records purpose, owner, license, maintenance/security posture,
   transitive impact, native build impact, and removal plan.
3. Preinstall/postinstall scripts are denied unless specifically reviewed and
   allowlisted.
4. New releases have a minimum adoption age except for an approved security
   response.
5. Electron/Chromium, Node LTS, Rust, MCP SDK, build toolchains, and connector
   vendor tools are reviewed at least monthly and before every release candidate.
6. Supported lines are upgraded before EOL. A major upgrade receives its own
   node, compatibility fixtures, rollback plan, and packaged smoke evidence.
7. Release artifacts include an SBOM, license notices, component digests, source
   revision, and reproducible build metadata.

## 6. Explicitly rejected shortcuts

- a browser/PWA for privileged hardware operations;
- Node, serial, filesystem, or shell access in the renderer;
- a separate MCP implementation per coding provider;
- a Rust MCP implementation while the required official Rust SDK is beta;
- loading the website inside a privileged Electron window;
- relying on Electron's embedded Node as the executable for external MCP clients;
- the experimental Electron Forge Vite plugin in the production build path;
- SQLite without protection for sensitive local records;
- a general shell command tool or in-process execution of repository build
  scripts;
- Docker Desktop, WSL, or a globally installed language runtime as an unannounced
  end-user prerequisite.

## 7. Foundation acceptance

`FND-002` is complete only when the chosen exact matrix:

- installs reproducibly on a clean supported Windows build;
- builds Electron main/preload/renderer, standalone MCP, and all Rust crates;
- passes strict TypeScript and Rust checks;
- produces no experimental-packaging dependency in the release path;
- records dependency licenses and executable/toolchain digests;
- passes a packaged smoke launch, runtime handshake, MCP current per-request/
  optional-discovery test, and the accepted provider-compatibility revision;
- is recorded in the release manifest and compatibility fixtures.
