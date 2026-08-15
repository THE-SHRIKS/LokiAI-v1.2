# Desktop UI Specification — LokiAI Main v1.2

Status: **Normative visual and interaction baseline**  
Updated: **2026-08-15**

## 1. Design objective

LokiAI Main is a dense hardware operations system that must feel calm, obvious,
and trustworthy. The accepted interface is preserved and connected to production
services. Production wiring must not turn it into a generic admin dashboard or
expose backend complexity directly to users.

The desktop should answer three questions immediately:

1. Is my runtime and hardware healthy?
2. What needs my attention or approval?
3. What are humans, agents, and LokiAI currently doing?

## 2. Desktop, not website

- The root lifecycle is `signed out → onboarding → workspace shell`, managed by
  desktop application state.
- Browser navigation/history is not the authoritative application router.
- Account sign-in opens the system browser; credentials are not collected inside
  an embedded privileged web page.
- Window close, tray, relaunch, protocol callback, notifications, and updates are
  native application events handled outside React.
- Renderer storage is not authoritative for sessions, onboarding, device state,
  jobs, or approvals.
- The app must function with explicit cloud/runtime disconnected states rather
  than silently becoming a browser preview.

## 3. Brand tokens

| Token | Hex | Usage |
|---|---|---|
| `--void-black` | `#010101` | root canvas |
| `--chamber-panel` | `#050505` | cards and primary surfaces |
| `--chamber-panel-raised` | `#0A0A08` | raised workspace containers |
| `--panel-header` | `#090909` | headers and secondary surfaces |
| `--popover` | `#0F0F0C` | menus, dialogs, inspectors |
| `--proof-cream` | `#E8D9A8` | primary foreground |
| `--muted-cream` | `#8A8577` | secondary labels and timestamps |
| `--validation-olive` | `#6B8C42` | focus, outlines, validated charts |
| `--olive-primary` | `#8AB55A` | primary CTA and active states |
| `--olive-live` | `#A8C878` | live/healthy/highlight state |
| `--chamber-border` | `#23221C` | dividers and subtle borders |
| `--destructive` | `#CC272E` | critical/error/destructive state |

Color never carries meaning alone. Statuses pair color with an icon and text.
Olive is not used for destructive confirmation merely to match the brand.

## 4. Application shell

### Sidebar

Top to bottom:

- LokiAI identity and version channel;
- active workspace selector with environment;
- Dashboard;
- Devices;
- Agents;
- Monitoring;
- Incidents with actionable count;
- Deployments;
- Activity;
- Settings;
- runtime status/version;
- account menu and logout.

### Top bar

Contains contextual breadcrumb/title, command/search entry, runtime/gateway health,
live/freshness state, approvals, and context-relevant primary actions. Actions that
cannot run are disabled with an explanation, never hidden when the user needs to
understand missing capability.

The action center also receives deduplicated notification intents. OS
notifications deep-link to the exact authorized LokiAI object; they contain no
secret, raw device identifier, unrestricted log, or operation capability.

### Content region

- Use an overview-first hierarchy.
- Keep the most urgent action/evidence above large charts or histories.
- Prefer inspector drawers for contextual detail and modals only for bounded
  decisions/actions.
- Avoid deeply nested full-page navigation for one object.
- Preserve workspace context when moving between routes.

## 5. Route contract

| Route | Primary decision | Required top-level content |
|---|---|---|
| Dashboard | What needs attention now? | workspace/runtime health, action center, summaries, live bounded evidence, active agent work |
| Devices | What is connected and safe to do? | inventory, identity/support/capabilities, health/freshness, runtime, versions, operations |
| Agents | What are agents and subagents doing? | connections, run hierarchy, phases, requests, associations, changes, outcomes |
| Monitoring | What is the runtime observing? | runtime health, signals/quality/gaps, rules, telemetry, bounded logs |
| Incidents | What broke and what evidence exists? | severity/state, scope, trigger, evidence, investigation, action, verification timeline |
| Deployments | What changed on which targets? | immutable artifact, targets, approval, stages, per-target results, observation, rollback |
| Activity | Who did what and what happened? | append-only filterable events, actors, targets, result/risk, correlations |
| Settings | How is this workspace controlled? | workspace, agents, runtime, permissions, monitoring, notifications, security, data |

## 6. Onboarding contract

The desktop onboarding flow is:

```text
Connect account → Waiting for browser → Workspace → Coding agent
→ Existing device or new project
→ Discovery + health OR project target
→ Review → Workspace dashboard
```

Requirements:

- one dominant decision per screen;
- visible progress and ability to resume;
- accurate distinction between waiting, running, failed, unavailable, and done;
- browser auth explains that the browser is intentional and how to retry/cancel;
- agent setup presents client-specific instructions generated from the same MCP
  endpoint/capability contract;
- device discovery identifies source connector and refresh behavior;
- health shows individual checks and evidence, not a decorative percentage;
- new-project branch does not pretend a device exists;
- review lists what is connected, deferred, unavailable, and permitted.

## 7. Global state presentation

Every data surface supports:

- `initial-loading` — skeleton/layout without invented values;
- `refreshing` — prior timestamped projection remains visible if safe;
- `empty` — valid query with no entities/events;
- `cloud-disconnected` — local data source and freshness shown;
- `runtime-disconnected` — device truth unavailable after freshness expires;
- `runtime-stale` — last-known device projection remains clearly timestamped but
  is never labeled current/healthy;
- `permission-denied` — explain required role/capability without leaking data;
- `version-incompatible` — exact components and update action;
- `partial` — per-source status when one gateway/stream fails;
- `error` — stable error code, correlation ID, retry/support action;
- `ready/live` — source and last update always recoverable from the UI.

Do not show `0`, “healthy,” “connected,” or “deployed” when the value is unknown.

## 8. Operations and approvals

Before any high-risk confirmation, show:

- requesting actor and associated run/incident;
- exact workspace/environment and target snapshot;
- operation and immutable artifact/config/model digest;
- risk and why;
- expected effect and downtime;
- preflight checks and timestamp;
- verification and rollback plan;
- approval expiry;
- explicit approve and deny actions.

After approval, the modal does not claim completion. It transitions to durable-job
tracking. Denial and expiry are first-class outcomes.

## 9. Agent task graph presentation

- Clearly distinguish LokiAI-verified job stages from agent-reported work phases.
- Show parent/child run relationships without implying every child has hardware
  authority.
- Each requested operation links to its approval/job/activity evidence.
- Failures show the failing phase and actionable evidence; they are not removed
  when a retry starts.
- Parallel visual branches correspond to real distinct run IDs.
- Incident work-intent delivery shows queued, delivered, acknowledged, failed,
  expired, or `user_action_required`. It never displays an active agent until a
  client actually acknowledges and supplies a run identity.

## 9.1 Build execution presentation

- A build card shows source revision, target, recipe/toolchain, containment state,
  bounded logs, output/provenance digest, and terminal result.
- UI text distinguishes `build succeeded` from `approved to flash` and
  `deployment verified`.
- Containment or output-validation failure cannot expose an override button that
  reruns the same recipe without isolation.
- A build may be cancelled only at a safe state and continues to show teardown or
  reconciliation progress after the process is stopped.

## 10. Charts, logs, and freshness

- Every chart displays metric/unit, time range, source, sampling/aggregation, last
  update, gaps, and thresholds.
- Never connect across missing data as if it were observed.
- Charts use bounded points and downsampling outside the renderer hot path.
- Logs are virtualized/paginated, safely rendered as text, bounded by query, and
  show source/correlation/redaction.
- Live mode can pause without losing the user's inspected range.

## 11. Accessibility and desktop quality

- Full keyboard navigation and visible olive focus ring with sufficient contrast.
- Semantic labels/roles for icon-only actions, charts, status chips, dialogs, and
  inspectors.
- Focus trap/restore for dialogs and drawers.
- Reduced-motion mode and no motion required to understand state.
- Text and layout remain usable at Windows scaling from 100% through 200% and at
  the defined minimum window size.
- Destructive/high-risk confirmation cannot be triggered by an ambiguous default
  Enter key state.
- Copyable IDs/correlation values are accessible without cluttering primary text.

## 12. UI acceptance

- The accepted current design remains visually recognizable.
- Every route consumes typed preload/application clients, not direct process or
  browser-global authority.
- No renderer session/device/job authority remains in browser storage.
- No successful-looking invented operational value remains.
- Runtime/offline notifications, incident delivery fallback, contained-build
  state, and device enrollment appear from real projections.
- Route states, onboarding, logout, approvals, relaunch, and disconnection pass the
  desktop E2E scenarios in `Docs/TESTING.md`.
