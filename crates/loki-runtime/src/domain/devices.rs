//! Device Registry and Connectors (`Docs/ARCHITECTURE.md` §7, bounded
//! context 3): device identity, capability, health, and connector metadata,
//! kept device-family-neutral per `AGENTS.md` product direction. Owned by
//! `CON-001` and later connector nodes. Not implemented by `RUN-001` — empty
//! on purpose; see `src/domain.rs`.
