# Graph Engineering Planner — LokiAI Main v1.2

Status: **Normative execution protocol**  
Updated: **2026-08-14**

## 1. What “graph engineering” means here

Graph engineering is the deliberate linking of intent, technical decisions,
implementation work, and proof. It is not a claim that many agents should edit at
once and it is not a requirement to add a graph framework to the product.

LokiAI maintains four connected graphs:

1. **Product graph:** goals, functional/non-functional requirements, and acceptance
   scenarios.
2. **Architecture graph:** components, boundaries, contracts, ADRs, and the
   requirements they satisfy.
3. **Execution graph:** bounded task nodes and dependency/ownership edges.
4. **Evidence graph:** changed files, tests, reviews, findings, approvals, hardware
   evidence, and releases.

Traceability is the link between these graphs. A task with no requirement is
unjustified; a release requirement with no passing evidence is incomplete.

## 2. Why this form is efficient

- The orchestrator holds global context; workers receive bounded task context.
- Only dependency-ready work is dispatched.
- Parallel workers have disjoint ownership, avoiding merge conflict and duplicate
  reasoning.
- Contracts are defined before both sides of an integration are implemented.
- Review is independent from implementation.
- Failure returns to the responsible node or invalid contract, not to the entire
  project.
- Human gates occur only where judgment or real-world risk requires them.

The optimization target is verified throughput, not maximum simultaneous agents.

## 3. Graph vocabulary

### Node types

| Type             | Purpose                                            | Typical owner              |
| ---------------- | -------------------------------------------------- | -------------------------- |
| `DECISION`       | Resolve a product/architecture ambiguity           | Architect + human          |
| `CONTRACT`       | Define schema, API, IPC, trait, or state machine   | Contract owner             |
| `IMPLEMENTATION` | Build one bounded behavior                         | Implementer                |
| `MIGRATION`      | Move structure or persisted data safely            | Implementer + integrator   |
| `VERIFICATION`   | Prove acceptance/security/compatibility            | Independent reviewer       |
| `HARDWARE`       | Produce evidence from a physical target            | Hardware operator + worker |
| `INTEGRATION`    | Combine ready outputs and run cross-boundary gates | Integrator                 |
| `RELEASE`        | Package, sign, and make a human release decision   | Release owner              |

### Edge types

| Edge         | Meaning                                                   |
| ------------ | --------------------------------------------------------- |
| `depends_on` | Target cannot start until source is accepted              |
| `consumes`   | Target reads the versioned artifact/contract from source  |
| `implements` | Code node implements a requirement or contract            |
| `validates`  | Verification node proves a source node                    |
| `reviews`    | Independent assessment of a source node                   |
| `owns`       | Node has exclusive write ownership of a file/contract set |
| `blocks`     | Open decision/failure prevents target readiness           |
| `supersedes` | Accepted decision/task replaces an earlier one            |
| `produces`   | Node emits an artifact/evidence required downstream       |

### Status values

`proposed`, `ready`, `running`, `review`, `blocked`, `failed`, `accepted`,
`cancelled`, `superseded`.

Only the orchestrator changes status. `accepted` means its exit criteria and
required evidence pass.

## 4. Task-node schema

Every execution node has:

```text
id                  stable domain-prefixed ID
title               one outcome, phrased as a result
type                one node type
status              one allowed status
objective           why this node exists
requirements        PRD requirement IDs
acceptance_scenarios SPEC acceptance scenario IDs
depends_on          accepted predecessor node IDs
consumes            exact contracts/artifacts and versions
owned_paths         exclusive writable paths/globs
read_context        smallest required docs/files
forbidden_scope     explicit non-goals
deliverables        concrete outputs
acceptance_criteria  binary, observable statements
verification        commands/scenarios/evidence
risk                 low/medium/high/critical
human_gate           none or named decision/approval
assignee             one accountable worker
reviewer             different worker/role when required
handoff_path         where result/evidence is recorded
```

Use `Docs/graph/templates/TASK_PACKET.md`. A task that cannot name binary acceptance
criteria is not ready for implementation.

## 5. Decomposition rules

A good node:

- produces one coherent outcome;
- can be understood with a bounded context packet;
- has one exclusive ownership set;
- can be verified independently;
- is small enough to review as one change;
- does not mix a mechanical move, a public contract change, and new behavior;
- does not require two workers to coordinate continuously.

Split a node when:

- two portions can run without shared files after a contract exists;
- it crosses security or process boundaries;
- it mixes architecture/decision with implementation;
- it has separate failure and rollback domains;
- review would be clearer as two changes.

Keep a node together when splitting would require constant cross-agent messaging
or both parts must edit the same central file.

## 6. Planning algorithm

### Step 1: Normalize intent

- Update PRD requirements and SPEC acceptance scenarios.
- Identify ambiguity that affects behavior, safety, compatibility, or scope.
- Create `DECISION` nodes for unresolved choices.
- Do not create implementation nodes whose acceptance depends on an unresolved
  decision.

### Step 2: Map architecture

- Identify affected bounded contexts, processes, schemas, and trust boundaries.
- Confirm locked decisions or create an ADR node.
- Assign a single owner to every changed contract.

### Step 3: Build the dependency DAG

- Create contract/decision nodes before their consumers.
- Add explicit dependencies; never rely on ordering in a list.
- Add independent verification and integration nodes.
- Detect cycles. A cycle means the tasks or contract ownership are wrong and must
  be decomposed.

### Step 4: Calculate the ready frontier

A node is ready only when:

- all `depends_on` nodes are accepted;
- required human decisions are recorded;
- its packet is complete;
- its owned paths do not overlap a running node;
- required environment/credentials/device are available or the node does not need
  them;
- acceptance and verification are executable.

### Step 5: Form a wave

Choose a small set of ready nodes with disjoint write sets. Prefer critical-path
nodes and contract producers. Do not fill every worker slot if ownership or
integration capacity would become the bottleneck.

### Step 6: Dispatch

- Give each worker the task packet, AGENTS rules, required documents, starting
  revision, and handoff path.
- Use isolated branches/worktrees after `FND-001` establishes Git.
- Name branch/worktree by node ID.
- The orchestrator remains responsible for questions, scope changes, and graph
  updates.

### Step 7: Worker self-verification

The worker runs node-level verification, inspects its diff, and produces a handoff.
A worker may report a failure; it may not weaken acceptance criteria to report
success.

### Step 8: Independent review

Review focuses on the task contract, architecture, security, tests, regressions,
and unexpected scope. Review findings link to the originating node and severity.

### Step 9: Integrate

- Rebase/update against the accepted integration baseline.
- Apply nodes one at a time in topological order.
- Run contract and affected integration tests after each node.
- Run the wave gate after all nodes are integrated.
- Update traceability and graph evidence.

### Step 10: Failure routing

Classify before acting:

- implementation defect → return to the same worker/node;
- missing test/evidence → return to node or create verification child;
- contract defect → mark affected consumers blocked and re-plan descendants;
- architecture conflict → create ADR/decision node;
- environment/device unavailable → `BLOCKED_DEPENDENCY`, continue independent
  frontier;
- integration conflict → integrator resolves only when behavior is unchanged;
  otherwise return to owner;
- security/release violation → stop affected branch and require named human gate.

After two targeted correction cycles for the same underlying failure, the
orchestrator must reconsider decomposition or assumptions instead of repeating the
same prompt.

## 7. Agent roles

### Orchestrator

- owns global context, graph, status, and scheduling;
- creates packets and prevents write collisions;
- answers/records decisions;
- receives handoffs and dispatches independent review;
- never claims implementation success without evidence;
- is the only agent allowed to update task status.

### Planner/architect

- read-only unless explicitly assigned documentation ownership;
- inspects repository and requirements;
- proposes nodes, edges, contracts, decisions, risks, and verification;
- does not implement the feature it plans in the same node when an independent
  implementer is available.

### Implementer

- owns one bounded node and its allowed paths;
- implements behavior/tests/docs required by the packet;
- does not edit graph status or adjacent subsystems;
- returns structured evidence.

### Test reviewer

- independently maps acceptance criteria to tests;
- runs relevant verification and inspects negative/error paths;
- may propose test changes only when assigned ownership;
- never reports pass based on worker narrative alone.

### Security reviewer

- examines trust boundaries, authorization, secret handling, input validation,
  side effects, dependency/update risk, and unsafe failure behavior;
- maps findings to `Docs/SECURITY.md` and severity;
- cannot waive a finding it authored.

### Integrator

- owns the integration branch/worktree;
- verifies starting revisions and handoffs;
- integrates in dependency order and resolves only mechanical conflicts;
- sends behavioral conflicts back to owners;
- runs cross-package and wave gates;
- updates evidence links, not product intent.

### Human product/security/release owner

- accepts ambiguous product decisions and locked ADRs;
- approves high-risk real-world operations;
- accepts residual security risk;
- makes merge/release/deployment decisions.

## 8. Context-packet policy

The orchestrator should pass:

- node ID/objective;
- exact requirements and acceptance criteria;
- dependencies and consumed contract versions;
- owned and forbidden paths;
- relevant architecture/security excerpts or file links;
- starting revision and known workspace state;
- verification and required handoff.

Do not pass the entire conversation, all research, every repo file, or other
workers' verbose logs unless directly required. A fresh worker should be able to
complete the node from the packet plus named files.

## 9. Ownership and concurrency

Use three ownership classes:

- **single-writer:** public contract, lockfile, root config, central migration,
  graph/status, release manifest;
- **package owner:** one worker may change files inside its package and tests;
- **append-only evidence:** workers add their own handoff/evidence record but do
  not rewrite another worker's record.

Default maximum wave size is three implementation workers plus the orchestrator,
reduced whenever integration/review is the bottleneck. Planner, researcher, and
reviewer tasks may run in parallel when read-only.

## 10. Gates

| Gate                       | Required evidence                                                                                                                                                                     | Human?                                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| G0 Intent                  | PRD/spec accepted; open decisions routed                                                                                                                                              | Yes                                                                                                                       |
| G1 Architecture            | process, contracts, threat model, ADRs accepted                                                                                                                                       | Yes for locked decisions                                                                                                  |
| G2 Node                    | task acceptance and node tests pass                                                                                                                                                   | No unless packet says so                                                                                                  |
| G3 Wave integration        | integrated build, contract, integration, review pass                                                                                                                                  | No                                                                                                                        |
| **P1 Part One completion** | `P1-INT-001` accepted — every W0–W6 node integrated and proven working together (not just individually accepted), per `Docs/graph/TASK_GRAPH.md` section 4 "Part One completion gate" | **Yes** — human approval to begin Part Two (W7–W10) is separate from and not implied by `P1-INT-001` technical acceptance |
| G4 Security                | release security matrix and independent review pass                                                                                                                                   | Yes for residual risk                                                                                                     |
| G5 Hardware                | required physical ESP32-S3 scenarios and recovery evidence pass                                                                                                                       | Yes/operator present                                                                                                      |
| G6 Release                 | signed artifacts, installer/update/rollback, traceability, release review                                                                                                             | Yes                                                                                                                       |

No downstream gate erases a failure from an earlier gate.

## 11. Graph maintenance

- Keep task IDs stable.
- Record newly discovered work as new nodes; do not hide it in handoff prose.
- When a node is superseded, preserve its history and point to the replacement.
- Update descendant readiness when dependencies change.
- Store evidence by immutable revision/artifact/test-run ID where available.
- Review the critical path after each wave.
- Archive finished wave details outside the always-loaded rules so agent context
  remains lean.

## 12. Program structure — Part One / Part Two

Recorded 2026-08-15 by explicit human instruction; authoritative copy lives in
`Docs/graph/TASK_GRAPH.md` section 1a. Part One is waves W0–W6 (core platform:
foundation, contracts, Electron shell, Rust runtime/IPC, persistence, auth/
onboarding services, connectors/enrollment, MCP/agent-run services, monitoring/
incidents/notifications, jobs/policy/approvals, contained build executor,
artifacts, device operations/deployment, cloud sync). Part Two is waves W7–W10
(UI wiring, `INT-001` full integration, `SEC-001`/`SEC-002` independent review,
packaging, `HIL-001`, release). The orchestrator plans and dispatches Part One
continuously without pausing after each node (Section 6 Step 5–6 apply per node as
usual, but do not wait for a human turn between nodes); it stops for a human
decision only where `Docs/CONTEXT.md`'s `OQ-001`–`OQ-007`, a missing credential/
environment, a physical device, or a named approval genuinely blocks the specific
branch — independent ready branches keep moving. Part Two dispatch requires both
`P1-INT-001` acceptance and a separate human go-ahead; do not infer the second from
the first.
