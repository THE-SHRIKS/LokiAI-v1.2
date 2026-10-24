# VM environment finding — `DAT-002-MEM-HOST-VERIFY-001`

Date: 2026-08-27 (session start ~16:23 local / ~2026-08-27T10:53:47Z per
`VBoxManage showvminfo`)

## Summary

The VirtualBox VM `LokiAI` (`fc16179d-3eb0-45e4-8e7e-69964861f820`), which
this task packet's Section 3 "Required environment" describes as
already fully provisioned (Guest Additions installed, guest-control
automation confirmed working, Rust+MSVC+Perl toolchain confirmed working,
`loki` primary account plus a second local account already present,
single-CPU as a firmware-bug workaround), is **not** currently in that
state. It is on its `clean-install-baseline` snapshot — described in the
VM's own snapshot metadata as *"Fresh Windows 11 install, before any user
accounts or LokiAI setup"* — and is configured with 4 CPUs, not 1.

This blocks every VM-execution-dependent deliverable in this task (Section 8
rows 3–10): there is no `loki` account, no second local account, no Rust/
MSVC/Perl toolchain, and (evidenced directly below) the VM did not reach a
guest-control-ready state at all during this session.

## Exact evidence

- `VBoxManage.exe list vms` → confirms `"LokiAI" {fc16179d-3eb0-45e4-8e7e-69964861f820}`
  is the correct, only VM matching the packet's name (a second VM,
  `LokiAI-BLD-Validation`, exists but is unrelated — 2 CPUs, no snapshots,
  a different validation VM — and was not touched).
- `VBoxManage.exe snapshot "LokiAI" list`:
  ```
  Name: clean-install-baseline (UUID: 20d2350b-a604-42ec-820c-4111f78b203b) *
  Description: Fresh Windows 11 install, before any user accounts or LokiAI setup
  ```
  This is the **only** snapshot that exists and is the **current** snapshot
  (`CurrentSnapshotName="clean-install-baseline"`). There is no later,
  post-provisioning snapshot to revert to instead.
- `VBoxManage.exe showvminfo "LokiAI" --machinereadable` (relevant fields):
  `cpus=4` (packet: "single-CPU ... do not change CPU count" — the
  packet's own asserted precondition is CPU count 1; the VM's actual
  configuration is 4).
- Started the VM headless (`startvm "LokiAI" --type headless`) and polled
  `GuestAdditionsRunLevel` every 10–15s for approximately 21 minutes total
  wall-clock (two consecutive polling windows, ~4 min and ~17 min). It never
  advanced past `GuestAdditionsRunLevel=1` (driver-level only — no
  userland/desktop service, the level `guestcontrol run` requires is `>=2`).
  Every `guestcontrol run` attempt during this window failed with:
  ```
  VBoxManage.exe: error: Guest Additions are not installed or not ready (yet)
  ```
  (later, once the guest execution service partially initialized:
  `Waiting for guest process failed: The guest execution service is not
  ready (yet)`).
- Two screenshots (`vm-boot-splash-t0.png`, taken immediately after start;
  `vm-boot-splash-t13min.png`, taken ~13 minutes later) both show the VM
  still on the VirtualBox EFI splash screen (spinner animation advanced
  only slightly between the two, confirming the VM was not frozen/crashed —
  see next point — but also confirming it never progressed past this
  screen to the Windows OOBE/desktop in that window).
- `Get-Process` on the three `VBoxHeadless.exe`-related PIDs after ~30
  minutes of elapsed wall time showed the main VM process
  (PID 9724) had accumulated **1603.8 CPU-seconds**, i.e. sustained ~89%
  average utilization of one core the entire time, while never leaving the
  splash screen. This is consistent with the exact failure mode the
  packet's own environment note names — *"single-CPU (a firmware bug
  workaround — do not change CPU count)"* — reproducing under the VM's
  actual, non-workaround 4-CPU configuration, though it is equally
  consistent with a stalled first-boot OOBE sequence on a genuinely fresh
  Windows 11 install (network/region/account-setup screens can spin for a
  long time headless with no interactive input). Both explanations point to
  the same root cause: **this VM is not in its expected provisioned state**,
  not to a transient flake.
- After confirming the above, the VM was cleanly powered off
  (`VBoxManage.exe controlvm "LokiAI" poweroff`, exit 0) to avoid leaving an
  unattended, resource-consuming, unauthorized-state VM running. No other
  VM (guest disk, snapshot, account, `C:\`, host registry/policy) was
  touched.

## What this node did *not* do, and why

- Did **not** change the VM's CPU count. The packet is explicit
  ("do not change CPU count") and, independently, changing VM hardware
  configuration is outside this node's `Forbidden`/ownership scope
  (`crates/loki-storage/src/secure_memory.rs`, two new test files, this
  handoff, and this evidence directory only) and outside the human gate's
  exact scope (fault-injection *on the already-provisioned VM*, not
  re-provisioning it).
- Did **not** attempt to run Windows OOBE, create the `loki` account or the
  second local account, or install Rust/MSVC/Perl inside the VM. That is a
  substantial, unscoped body of environment-setup work this task packet
  assumes is already done; redoing it here would silently substitute a
  different (and much larger, differently-risked) task for the one this
  packet describes, which `AGENTS.md` "Task hygiene" explicitly prohibits
  ("Confirm prerequisites exist. If they do not, return `BLOCKED_DEPENDENCY`
  ... instead of creating a substitute implementation").
- Did **not** attempt any elevation, UAC interaction, or automated OOBE
  click-through, all of which are outside this node's authorized scope even
  if guest-control had been reachable.

## Conclusion

The environment precondition in Section 3 ("Required environment") is not
met. This is a `BLOCKED_DEPENDENCY` for every VM-execution-dependent
deliverable (Section 8 rows 3–10 and the Section 7 acceptance criteria that
depend on them). The code-only and authorable-without-a-VM parts of this
node (Deliverable 1; the two new test files) were completed and verified on
the host in this same session — see the handoff for exact results — so the
next dispatch, once the VM is confirmed actually re-provisioned (or a
replacement environment is authorized), should be able to proceed directly
to Section 8 rows 3–10 without repeating this investigation.
