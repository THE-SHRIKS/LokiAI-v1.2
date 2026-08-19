# Task Packet — `RUN-001-CORR-001 Restore Runtime Stdout Diagnostics`

Packet status: `accepted`  
Graph status: `running`  
Created by: `orchestrator`  
Created at: `2026-08-19T09:18:50Z`  
Starting revision: `76c73a14e5cc9abb2efe01b4d9e087fb175d40fb`

## 1. Objective

Restore RUN-001's documented structured-stdout process contract after the
Part One workspace gate proved that `loki-runtime --health-check` exited zero
without an observable lifecycle record. Make the writer explicit and isolate
black-box default-logging assertions from an ambient operator `RUST_LOG`
override without changing production filtering semantics.

## 2. Traceability

- Requirements: `NFR-OBS-001`, `NFR-SUP-001`
- Acceptance scenarios: runtime startup/health and graceful process lifecycle
- Architecture components: Rust runtime diagnostics and process boundary
- Security controls: log redaction remains owned by the existing diagnostics contract
- Parent/feature/incident: `RUN-001`; regression discovered by `INC-001` workspace verification

## 3. Dependencies and inputs

- `depends_on`: `RUN-001`, `RUN-003`
- Consumed contracts and exact versions/revisions:
  - RUN-001 structured-diagnostics contract at starting revision
- Required environment/credential/device:
  - Windows named-pipe/process test environment; no credentials or hardware
- Known starting conditions:
  - `loki-runtime --health-check` exits zero but its INFO lifecycle event is
    written to stderr by the library default, while the process contract and
    black-box test require stdout.
  - The developer environment can set `RUST_LOG=warn`; black-box tests of the
    documented default must not inherit that override.

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`
- `Docs/TESTING.md`
- `Docs/graph/handoffs/RUN-001-1.md`
- `Docs/graph/handoffs/RUN-003-1.md`
- `crates/loki-runtime/src/logging.rs`
- `crates/loki-runtime/tests/process_behavior.rs`
- `crates/loki-runtime/tests/support/mod.rs`

## 5. Ownership

Writable paths:

- `crates/loki-runtime/src/logging.rs`
- `crates/loki-runtime/tests/process_behavior.rs`
- `crates/loki-runtime/tests/support/mod.rs`
- `Docs/graph/tasks/RUN-001-CORR-001.md`
- `Docs/graph/handoffs/RUN-001-CORR-001-1.md`

Read-only dependency paths:

- all other workspace files

Forbidden scope:

- changing runtime filtering, redaction, IPC, persistence, or shutdown behavior;
- adding dependencies or weakening tests.

No worker may expand ownership without orchestrator acceptance.

## 6. Deliverables

1. Explicit JSON diagnostics writer matching the stdout process contract.
2. Deterministic black-box tests of default logging behavior.
3. Real runtime and IPC process-integration verification.
4. Handoff/evidence record.

## 7. Acceptance criteria

- [ ] `--health-check` exits zero and emits its structured lifecycle record on stdout.
- [ ] Run-mode child-process tests observe default INFO lifecycle events even
      when the parent environment has a stricter `RUST_LOG` override.
- [ ] Production `RUST_LOG` filtering remains available and unchanged.
- [ ] All runtime and IPC unit/process integration tests pass.
- [ ] Failure/unavailable behavior is explicit and tested.
- [ ] No prohibited security or architecture boundary is weakened.
- [ ] No unrelated files change.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt -p loki-runtime -- --check` | exit 0 | command/result |
| Static | `cargo clippy -p loki-runtime --all-targets -- -D warnings` | exit 0 | command/result |
| Unit/integration | `cargo test -p loki-runtime --lib --tests --quiet` | all tests pass | counts/result |
| Process integration | `cargo test -p loki-runtime --test process_behavior --quiet` | real child-process and named-pipe cases pass | counts/result |
| IPC regression | `cargo test -p loki-ipc --lib --tests --quiet` | all tests pass | counts/result |
| Hardware/cloud/desktop/containment | n/a | no behavior in these layers changes | handoff statement |
| Security | diff review for filtering/redaction/trust-boundary change | no blocking finding | review result |

If a mandatory row cannot run, the node result cannot be `COMPLETE`.

## 9. Risks and safety

- Risk level: `medium`
- Failure modes: diagnostics on the wrong stream; ambient filters causing false
  negative tests; stale child process retaining the well-known named pipe.
- Rollback/recovery: revert the isolated correction commit.
- Data/secret impact: no new data; existing redaction/filtering retained.
- Executable/toolchain impact: none.
- Cloud/offline impact: none.
- Device impact: none.

## 10. Human gate

- Gate: `none`
- Exact decision/operation requiring approval: none
- Approver role: n/a
- Approval evidence: n/a

## 11. Worker and reviewer

- Accountable worker: `Codex orchestrator correction worker`
- Independent reviewer: `orchestrator diff/security audit`
- Worktree/branch: `target/worktrees/run-logging-fix` / `codex/run-logging-fix`
- Handoff record: `Docs/graph/handoffs/RUN-001-CORR-001-1.md`

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` or `NEEDS_REPLAN` if:

- the output-stream change breaks an accepted consumer contract;
- required process integration cannot run on Windows;
- correction requires expanding into IPC, persistence, or lifecycle behavior;
- acceptance cannot be verified without weakening the contract.
