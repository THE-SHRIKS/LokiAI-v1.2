# LokiAI v1.2

**Device-neutral infrastructure for AI-assisted physical systems.**

LokiAI is an engineering project to connect authorized AI agents and developer
tools to physical devices through explicit capabilities, policy boundaries,
observable state, and verifiable device operations. Its first reference target
is the **ESP32-S3**; the wider architecture is intended to support additional
microcontrollers and device families without embedding ESP32-specific assumptions
in shared contracts.

## Repository contents

- `crates/loki-connectors-sdk/`: device-neutral connector interfaces and contracts.
- `crates/loki-connector-esp32-s3/`: ESP32-S3 reference connector structure.
- `crates/loki-runtime/` and related crates: runtime, authority, monitoring,
  approvals, and other supporting components.
- `services/mcp-server/`: agent-facing MCP service foundation.
- `LokiAI Desktop UI/`: desktop renderer application.
- `Docs/`: architecture, engineering specifications, limitations, and evidence.
- `AGENTS.md`: project-wide engineering and safety rules.

## Status and safety

**Development-stage software.** Code-level connector structures are not the
same as certified physical device integration. In particular, the ESP32-S3
reference connector in this repository documents a transport abstraction;
physical I/O, real-device validation, and certification require separate work
and evidence. See `crates/loki-connector-esp32-s3/Cargo.toml` and
`Docs/CONNECTOR_SPEC.md` for scope and constraints.

Do not treat this repository as proof of production-ready robotics or industrial
hardware control. Device-changing operations must be explicitly authorized,
validated, and verified on real hardware before claiming success.

## Development

The repository uses a Rust/Cargo and TypeScript/pnpm workspace. Follow
`AGENTS.md` and `Docs/TECH_STACK.md` for pinned toolchain versions and
`Docs/TESTING.md` for verification requirements. The root `package.json`
contains the available scripts. Dependency installation, builds, and tests
may require platform-specific tools; no build or hardware certification is
asserted by this README.

## License

Copyright 2026 THE SHRIKS.

LokiAI v1.2 is licensed under the **Apache License, Version 2.0**.
See [LICENSE](LICENSE) for the full text and [NOTICE](NOTICE) for attribution
and third-party notice guidance. Bundled or referenced third-party material
remains subject to its own terms; see `Docs/RELEASE.md` for release-specific
notice requirements.

The repository is public source code; individual packages marked `private: true`
or `publish = false` are not configured for registry publication. Those
packaging flags do not change the repository's Apache-2.0 license.
