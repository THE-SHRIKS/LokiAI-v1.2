import { z } from 'zod';

/**
 * State enums transcribed from `Docs/SPEC.md` Section 4 state machines — values
 * are not invented; every literal below is the exact vocabulary the document
 * defines. Two casing conventions exist in the source document itself:
 *
 * - Machines given as a plain-text "States: ..." list (Runtime connection, Job,
 *   Deployment) or a plain-text arrow flow (Onboarding, Device lifecycle) use
 *   `lowercase-hyphenated` tokens verbatim in `Docs/SPEC.md`.
 * - Machines given only as a Mermaid `stateDiagram-v2` (Incident) use
 *   PascalCase state *labels*, which is Mermaid's own rendering convention, not
 *   a distinct wire format: the same conceptual state
 *   (`WaitingApproval` in the Incident diagram) is spelled `waiting-approval`,
 *   verbatim, in two sibling plain-text lists in the very same document
 *   (Section 4.5 Agent run, Section 4.7 Durable job). This package normalizes
 *   Mermaid-only state machines to the document's own `lowercase-hyphenated`
 *   wire convention so every enum in this package shares one casing style; the
 *   *set* and *meaning* of states is unchanged from the diagram. See
 *   `packages/contracts/README.md` for the full mapping and
 *   `Docs/graph/handoffs/CTR-001-1.md` for the reasoning trail.
 */

// --- 4.3 Runtime connection ---------------------------------------------------------

/** `Docs/SPEC.md` Section 4.3 Runtime connection. Plain-text list, used verbatim. */
export const RUNTIME_CONNECTION_STATES = [
  'unknown',
  'starting',
  'connected',
  'degraded',
  'disconnected',
  'version-incompatible',
  'unauthorized',
  'updating',
  'stopping',
] as const;
export const RuntimeConnectionStateSchema = z.enum(RUNTIME_CONNECTION_STATES);
export type RuntimeConnectionState = z.infer<typeof RuntimeConnectionStateSchema>;

// --- 4.7 Durable job -----------------------------------------------------------------

/** `Docs/SPEC.md` Section 4.7 Durable job. Plain-text list, used verbatim. */
export const JOB_STATES = [
  'queued',
  'policy-check',
  'waiting-approval',
  'preflight',
  'executing',
  'restarting',
  'testing',
  'observing',
  'succeeded',
  'failed',
  'cancelled',
  'rollback-pending',
  'rolling-back',
  'rolled-back',
  'outcome-unknown',
] as const;
export const JobStateSchema = z.enum(JOB_STATES);
export type JobState = z.infer<typeof JobStateSchema>;

// --- 4.8 Deployment --------------------------------------------------------------------

/** `Docs/SPEC.md` Section 4.8 Deployment. Plain-text list, used verbatim. */
export const DEPLOYMENT_STATES = [
  'draft',
  'validating',
  'approval-required',
  'scheduled',
  'preflight',
  'transferring',
  'applying',
  'restarting',
  'testing',
  'observing',
  'verified',
  'completed',
  'failed',
  'rollback-required',
  'rolling-back',
  'rolled-back',
  'cancelled',
] as const;
export const DeploymentStateSchema = z.enum(DEPLOYMENT_STATES);
export type DeploymentState = z.infer<typeof DeploymentStateSchema>;

// --- 4.6 Incident (normalized from Mermaid PascalCase; see module doc) --------------

/**
 * `Docs/SPEC.md` Section 4.6 Incident `stateDiagram-v2` labels, normalized to
 * this package's `lowercase-hyphenated` wire convention:
 * `Open`→`open`, `Investigating`→`investigating`,
 * `WaitingApproval`→`waiting-approval`, `Executing`→`executing`,
 * `Verifying`→`verifying`, `Resolved`→`resolved`, `Closed`→`closed`.
 */
export const INCIDENT_STATES = [
  'open',
  'investigating',
  'waiting-approval',
  'executing',
  'verifying',
  'resolved',
  'closed',
] as const;
export const IncidentStateSchema = z.enum(INCIDENT_STATES);
export type IncidentState = z.infer<typeof IncidentStateSchema>;

/**
 * Legal transitions transcribed edge-for-edge from the Section 4.6 Mermaid
 * diagram (including the two self-referential recovery edges: `Verifying` back
 * to `Investigating`, and `Resolved` back to `Open` on reopen).
 */
export const INCIDENT_TRANSITIONS: Readonly<Record<IncidentState, readonly IncidentState[]>> = {
  open: ['investigating'],
  investigating: ['waiting-approval', 'verifying'],
  'waiting-approval': ['investigating', 'executing'],
  executing: ['verifying', 'investigating'],
  verifying: ['resolved', 'investigating'],
  resolved: ['open', 'closed'],
  closed: [],
};

// --- 4.2 Onboarding --------------------------------------------------------------------

/**
 * `Docs/SPEC.md` Section 4.2 Onboarding checkpoint steps, transcribed from the
 * plain-text flow:
 * `account -> workspace -> agent -> device-choice -> existing-device ->
 * discovery -> health -> review`, `-> new-project -> project-target -> review`,
 * `review -> ready`.
 */
export const ONBOARDING_STEPS = [
  'account',
  'workspace',
  'agent',
  'device-choice',
  'existing-device',
  'discovery',
  'health',
  'new-project',
  'project-target',
  'review',
  'ready',
] as const;
export const OnboardingStepSchema = z.enum(ONBOARDING_STEPS);
export type OnboardingStep = z.infer<typeof OnboardingStepSchema>;

/**
 * Legal forward transitions transcribed edge-for-edge from the Section 4.2
 * flow. `device-choice` branches into the existing-device path or the
 * new-project path; both converge on `review`, which either loops back (back
 * navigation, `Docs/SPEC.md` 4.2 "Back navigation is allowed unless it would
 * invalidate an already executing job") or advances to `ready`.
 */
export const ONBOARDING_TRANSITIONS: Readonly<Record<OnboardingStep, readonly OnboardingStep[]>> = {
  account: ['workspace'],
  workspace: ['agent'],
  agent: ['device-choice'],
  'device-choice': ['existing-device', 'new-project'],
  'existing-device': ['discovery'],
  discovery: ['health'],
  health: ['review'],
  'new-project': ['project-target'],
  'project-target': ['review'],
  review: ['ready'],
  ready: [],
};

// --- 4.4 Device lifecycle --------------------------------------------------------------

/**
 * `Docs/SPEC.md` Section 4.4 Device lifecycle, transcribed from the ASCII
 * diagram. Included even though the CTR-001 task packet's Deliverable 4 names
 * only "connection, job, incident, deployment, onboarding" because Deliverable
 * 5 (connector capability schema shapes) needs a closed, SPEC-derived type for
 * a capability's declared `required_state` — see `./connector.ts`. Not
 * invented: every value below is one of the eleven ASCII-diagram node labels.
 *
 * Deliberately NOT included: a transition-edge map. Unlike Incident (explicit
 * Mermaid `A --> B` statements) and Onboarding (linear text arrows), the
 * Section 4.4 diagram encodes branches through column-aligned vertical bars
 * (`|`, `v`) whose exact source/target pairing depends on ASCII column
 * position. That alignment is not reliably machine-parseable from prose
 * without risking a fabricated edge — see AGENTS.md's prohibition on
 * inventing product behavior. A future node adding this transition map should
 * confirm the exact edges with the diagram's author rather than infer them.
 */
export const DEVICE_LIFECYCLE_STATES = [
  'discovered',
  'identifying',
  'identified',
  'checking',
  'enrollment-required',
  'unsupported',
  'degraded',
  'enrolled',
  'ready',
  'offline',
  'busy',
] as const;
export const DeviceLifecycleStateSchema = z.enum(DEVICE_LIFECYCLE_STATES);
export type DeviceLifecycleState = z.infer<typeof DeviceLifecycleStateSchema>;
