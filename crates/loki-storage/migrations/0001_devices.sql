-- DAT-001: device registry table.
--
-- Docs/SPEC.md 3.3 Device: "Required fields: device_id, connector-qualified
-- hardware identity, family, board/module identity where verified, connector
-- ID/version, transport reference, runtime ID, capability set, support
-- tier, firmware/config/model artifact references when observable, health
-- projection, last seen, and lifecycle state."
--
-- capabilities_json / health_json / *_artifact_ref are stored as opaque
-- TEXT (already-serialized JSON, or an opaque reference string) because
-- their internal shape is owned by Docs/CONNECTOR_SPEC.md / CON-001, which
-- is out of this node's required-reading scope
-- (Docs/graph/tasks/DAT-001.md §4). This table guarantees the field is
-- durably persisted and retrievable verbatim, not what is inside it —
-- Deliverable 3 explicitly scopes this crate to "exercise the pattern for
-- later nodes to extend," not model every entity's full internal shape.
--
-- CHECK constraints on support_tier/lifecycle_state are defense-in-depth
-- against a hand-edited or externally-corrupted row; the primary trust
-- boundary is the Rust-side enum validation in src/states.rs
-- (AGENTS.md rule 4: "Types alone are not runtime validation" applies
-- symmetrically — a DB constraint alone is not application-level
-- validation either, so both layers enforce the same closed vocabulary).
CREATE TABLE IF NOT EXISTS devices (
    device_id             TEXT NOT NULL PRIMARY KEY,
    connector_id           TEXT NOT NULL,
    connector_version      TEXT NOT NULL,
    hardware_identity      TEXT NOT NULL,
    family                 TEXT NOT NULL,
    board_identity         TEXT,
    transport_reference    TEXT,
    runtime_id             TEXT NOT NULL,
    support_tier           TEXT NOT NULL CHECK (support_tier IN ('certified', 'supported', 'custom', 'unsupported')),
    lifecycle_state         TEXT NOT NULL CHECK (lifecycle_state IN (
                                'discovered', 'identifying', 'identified', 'checking',
                                'enrollment-required', 'unsupported', 'degraded', 'enrolled',
                                'ready', 'offline', 'busy'
                            )),
    capabilities_json      TEXT,
    firmware_artifact_ref   TEXT,
    config_artifact_ref     TEXT,
    model_artifact_ref      TEXT,
    health_json             TEXT,
    last_seen_at            TEXT NOT NULL,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
) STRICT;

-- A device's connector-qualified hardware identity must be unique within
-- that connector's namespace: two rows must never claim to be the same
-- physical hardware under the same connector.
CREATE UNIQUE INDEX IF NOT EXISTS idx_devices_connector_hardware
    ON devices (connector_id, hardware_identity);

CREATE INDEX IF NOT EXISTS idx_devices_runtime_id ON devices (runtime_id);
CREATE INDEX IF NOT EXISTS idx_devices_lifecycle_state ON devices (lifecycle_state);
