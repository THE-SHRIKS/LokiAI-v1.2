# Main-branch source reconciliation, 24 October 2026

Human instruction: reconcile conflicting branch versions into active code while retaining the current security model and contracts. Main baseline: 683f79d. Backup branches remain available.

## Resolution policy and evidence

- The active implementation remains the latest accepted LokiAI-v1.2 implementation. Historical task versions are not applied wholesale over subsequent corrections.
- Eleven original task branches contain only patch-equivalent commits already applied to main (git cherry against the baseline returns only minus entries). Their ancestry can be recorded without changing active code.
- BLD-001-R2-CODE, CON-003-R1, CON-002-R1-2 and CON-SDK-IDENTITY-EVIDENCE-001 source additions are already byte-identical to active main. Keep current Cargo members/lockfile, which additionally register later crates.
- CON-ID-CONTRACT-001 identity implementation and redaction are already present. Keep main connector tests with the later lint correction (W6 integration record), not the older redundant type assertion.
- DAT-002-CORR-001 was explicitly returned as NEEDS_REPLAN in TASK_GRAPH.md (SQLCipher secure-memory lifecycle failure). The accepted ADR-0005 secure-arena implementation supersedes that rejected candidate. Retain the current secure-memory guards, wipe logic, transaction API and storage dependencies; do not replace them with the older candidate APIs.
- DAT-002-MEM-CODE-001 was already integrated in 1d6ed5b/5c58938. Keep later approvals, mutation, runtime-service and native-credential composition. Its older branch would remove those changes.
- P1-RPC-CONTRACT-001 was explicitly rejected at REV-004 and superseded by P1-RPC-CONTRACT-001-R1 (f32a79e), followed by shutdown-drain and real runtime composition fixes. Retain the accepted R1 wire vocabulary, actor/parameter binding, exhaustive mutation mapping, dynamic revocation and draining behavior; do not restore rejected cooperative-mutation authority.
- P1 mutation-binding snapshot predates the current OperationBinding API. Keep actor_id and parameters_digest in canonical digests, current binding-version/grant checks and private immutable binding fields. Do not reintroduce the older constructor or remove the later approval controls.
- P1-AGENT-MUTATION-IDEMP-001 finish_run change is superseded by the transaction-bound terminal correction integrated in 196165b/94d80ec. Keep the corrected implementation. Port its four missing real SQLCipher crash/restart composition tests to the current immutable request constructor.
- P1-APPROVAL-JOB-ACCEPT-001 production behavior is already present in newer form. Port its missing changed-operation/cancellation coverage using real ApprovalStore creation and the same actual job/storage, rather than the older mirrored-job test.
- DAT-002-MEM-HOST-VERIFY-001 contributes a test-support-only candidate-arena helper and host-bound scenarios. Keep ACCEPTED_PROFILE=None and require_ready() unchanged. Mark host-bound tests explicitly ignored by default because they require the previously approved disposable environment and serial execution. This integration does not run a VM, alter host policy, choose production bounds, or accept the deferred verification node.
- Recover missing historical handoffs/evidence without overwriting newer documents. See recovered-documents.json.

## Exact source differences

- Test-only helper in crates/loki-storage/src/secure_memory.rs; module remains cfg(test)/test-support gated.
- crates/loki-storage/tests/secure_arena_host_bounds.rs retained as explicit opt-in environment tests.
- Four crash/restart composition tests ported to crates/loki-acceptance-store/tests/agent_mutation_composition.rs.
- Two meaningful approval/cancellation integration regressions added in acceptance_flow.rs.
- Existing workspace crates added only as dev-dependencies for those cross-crate tests; lockfile dependency lists updated. No production dependency or public wire contract changes.

## Branch resolution register

| Branch | Resolution |
|---|---|
| `backup/2026-10-24/CON-003-R1` | Source already integrated; retain current manifests/lockfile and later lint fixes. |
| `backup/2026-10-24/DAT-002-MEM-HOST-VERIFY-001` | Port compatible test helper and explicit opt-in host scenarios; preserve production gates. |
| `backup/2026-10-24/codex/bld-001-r2-code` | Source already integrated; retain current manifests/lockfile and later lint fixes. |
| `backup/2026-10-24/codex/con-002-r1` | Source already integrated; retain current manifests/lockfile and later lint fixes. |
| `backup/2026-10-24/codex/con-002-r1-2` | Source already integrated; retain current manifests/lockfile and later lint fixes. |
| `backup/2026-10-24/codex/con-id-contract-001` | Source already integrated; retain current manifests/lockfile and later lint fixes. |
| `backup/2026-10-24/codex/con-sdk-identity-evidence-001` | Source already integrated; retain current manifests/lockfile and later lint fixes. |
| `backup/2026-10-24/codex/dat-002-corr-001` | Superseded rejected candidate; retain accepted secure-arena/storage contracts. |
| `backup/2026-10-24/codex/dat-002-mem-code-001` | Accepted implementation already integrated; retain subsequent service and approval corrections. |
| `backup/2026-10-24/codex/p1-mutation-binding-corr` | Retain current stronger OperationBinding fields and approval checks. |
| `backup/2026-10-24/codex/p1-rpc-contract-001` | Superseded by accepted R1 and subsequent runtime/shutdown corrections; preserve current contracts. |
| `codex/AGT-001` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/AGT-002` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/APR-001` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/DAT-002` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/LDEV-001` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/LNOT-001` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/LOC-001` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/LONB-001` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/P1-DAT-INTEGRATION` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/dat-002-corr-001` | Superseded rejected candidate; retain accepted secure-arena/storage contracts. |
| `codex/p1-agent-terminal-corr` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/p1-operation-binding-corr` | Already applied with patch-equivalent commits; retain newer main implementation. |
| `codex/p1-rpc-contract-001` | Superseded by accepted R1 and subsequent runtime/shutdown corrections; preserve current contracts. |
| `node/P1-AGENT-MUTATION-IDEMP-001` | Keep newer transaction-bound finish_run; port and adapt missing composition tests. |
| `node/P1-APPROVAL-JOB-ACCEPT-001` | Keep current actor/parameters-bound approval behavior; port strengthened regressions. |

## Validation

The execution results and limitations are recorded separately in verification.md. No physical deployment, signing, VM provisioning or previously deferred host certification is claimed.
