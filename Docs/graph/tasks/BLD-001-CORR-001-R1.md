# Task Packet — `BLD-001-CORR-001-R1 Headless Win32k Enforcement`

Packet status: `accepted` by the human product owner's 2026-08-21 direction to
continue BLD-001 without weakening host security or relying on a VM.
Supersedes: `BLD-001-CORR-001.md` only for desktop/UI isolation. The original
packet remains immutable historical context.

## Objective

Close BLD-001's remaining interactive-desktop gap without creating a private
Window Station/Desktop and without changing host virtualization/security
settings. The production AppContainer build child must be created with Windows'
Win32k system-call disable mitigation. If that mitigation cannot be installed
at process creation, containment setup fails before the child is resumed.

This preserves every existing AppContainer, Job Object, filesystem ACL,
no-network, pipe, resource-limit, process-tree, and teardown control. It does
not permit a fallback to an unrestricted child process.

## Why this revision exists

The original correction's Window Station/Desktop method was directly probed
outside Codex in a normal PowerShell session and `CreateWindowStationW` failed
with `ERROR_ACCESS_DENIED` (Win32 error 5). A disposable VirtualBox VM was then
created from the official Windows ISO, but could not boot while preserving the
host's active Windows hypervisor/security configuration. The human owner
explicitly rejected disabling that protection. The build-execution requirement
is headless/no UI, not a particular Window Station implementation.

## Required production control

Use `PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY` at child-process creation with
`PROCESS_CREATION_MITIGATION_POLICY_WIN32K_SYSTEM_CALL_DISABLE_ALWAYS_ON`.
The mitigation must be part of the actual `STARTUPINFOEXW` used by the
suspended AppContainer child. It must not be test-only, optional by environment
variable, or applied after process creation.

## Ownership

Writable: `crates/loki-build-executor/**`,
`Docs/graph/handoffs/BLD-001-CORR-001-R1-1.md`, and a new dated append-only
section in `Docs/graph/handoffs/BLD-001-1.md`.

Read-only: all accepted packets, including `BLD-001.md` and
`BLD-001-CORR-001.md`; all other workspace files.

## Acceptance criteria

1. The production sandbox child receives the Win32k-disable mitigation before
   `CreateProcessW`; setup fails closed if the attribute cannot be installed.
2. A separately compiled hostile fixture runs through the production sandbox
   path and attempts real UI actions (window creation/enumeration, clipboard,
   input, capture, and desktop/window-station operations). Its evidence shows
   every applicable Win32k-dependent call is denied or unavailable under the
   mitigation; no mock, synthetic result, or test-only path is accepted.
3. The fixture's child/grandchild remains in the existing Job Object and no
   previously proven containment property regresses.
4. Deliberately preventing mitigation-attribute installation proves the child
   is not resumed and all partial resources are torn down; the fault is removed
   before final verification.
5. Fix the existing F-2 partial `clear_inherit` cleanup gap with a
   discriminating regression test.
6. Run all BLD-001 static, recipe-contract, containment, integration, and
   drive-canary verification. Record exact commands/results and any mandatory
   non-runnable check honestly.
7. Obtain an independent security review of the production launch path before
   proposing acceptance. Do not merge or mark BLD-001 accepted in this packet.

## Required reading

`AGENTS.md`, `Docs/BUILD_EXECUTION.md`, `Docs/SECURITY.md`,
`Docs/TESTING.md`, `BLD-001.md`, `BLD-001-CORR-001.md`, the complete existing
BLD handoff, and the actual Windows sandbox/fixture/test sources.

## Stop conditions

Return `NEEDS_REPLAN` if Windows cannot apply the mitigation to the real
AppContainer child, if any existing containment control must be weakened, or
if a real adversarial test cannot be run. Do not change host virtualization,
Windows security settings, ACLs on existing drives, or unrelated worktrees.
