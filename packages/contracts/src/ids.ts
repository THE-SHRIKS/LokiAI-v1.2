import { z } from 'zod';
import { digestSchema, idSchema } from './brand.js';

/**
 * Versioned, branded identifier schemas for every core entity named in
 * `Docs/SPEC.md` Section 3, plus the cross-cutting envelope identifiers
 * (`message_id`, `correlation_id`, `causation_id`) and content digests used
 * throughout `Docs/SPEC.md` Section 1 and `Docs/MCP_SPEC.md` Section 7.
 *
 * Every schema is exported alongside its inferred TypeScript type so callers
 * get both compile-time nominal typing and a runtime validator from the same
 * declaration — see `AGENTS.md` rule 4.
 */

// --- Section 3 entity identifiers -------------------------------------------------

/** `Docs/SPEC.md` 3.1 Workspace. */
export const WorkspaceIdSchema = idSchema('WorkspaceId');
export type WorkspaceId = z.infer<typeof WorkspaceIdSchema>;

/** `Docs/SPEC.md` 3.2 Runtime installation. */
export const RuntimeIdSchema = idSchema('RuntimeId');
export type RuntimeId = z.infer<typeof RuntimeIdSchema>;

/** `Docs/SPEC.md` 3.3 Device. */
export const DeviceIdSchema = idSchema('DeviceId');
export type DeviceId = z.infer<typeof DeviceIdSchema>;

/**
 * `Docs/SPEC.md` 3.3 "connector ID/version" and `Docs/CONNECTOR_SPEC.md`
 * Section 3 "connector ID and semantic version". Identifies the connector
 * implementation, not a specific device.
 */
export const ConnectorIdSchema = idSchema('ConnectorId');
export type ConnectorId = z.infer<typeof ConnectorIdSchema>;

/** `Docs/SPEC.md` 3.4 Agent connection. */
export const AgentConnectionIdSchema = idSchema('AgentConnectionId');
export type AgentConnectionId = z.infer<typeof AgentConnectionIdSchema>;

/** `Docs/SPEC.md` 3.4 Agent run. */
export const AgentRunIdSchema = idSchema('AgentRunId');
export type AgentRunId = z.infer<typeof AgentRunIdSchema>;

/** `Docs/SPEC.md` 3.6 Monitoring rule. */
export const MonitoringRuleIdSchema = idSchema('MonitoringRuleId');
export type MonitoringRuleId = z.infer<typeof MonitoringRuleIdSchema>;

/** `Docs/SPEC.md` 3.7 Incident. */
export const IncidentIdSchema = idSchema('IncidentId');
export type IncidentId = z.infer<typeof IncidentIdSchema>;

/** `Docs/SPEC.md` 3.8 Artifact. */
export const ArtifactIdSchema = idSchema('ArtifactId');
export type ArtifactId = z.infer<typeof ArtifactIdSchema>;

/** `Docs/SPEC.md` 3.8 Deployment. */
export const DeploymentIdSchema = idSchema('DeploymentId');
export type DeploymentId = z.infer<typeof DeploymentIdSchema>;

/** `Docs/SPEC.md` 3.9 Approval. */
export const ApprovalIdSchema = idSchema('ApprovalId');
export type ApprovalId = z.infer<typeof ApprovalIdSchema>;

/** `Docs/SPEC.md` 3.10 Activity event. */
export const ActivityIdSchema = idSchema('ActivityId');
export type ActivityId = z.infer<typeof ActivityIdSchema>;

/** `Docs/SPEC.md` 3.11 Device enrollment. */
export const EnrollmentIdSchema = idSchema('EnrollmentId');
export type EnrollmentId = z.infer<typeof EnrollmentIdSchema>;

/**
 * `Docs/SPEC.md` 3.11 "one-time cloud challenge reference" — distinct from the
 * enrollment record's own `enrollment_id` per AGENTS.md rule 5 (idempotency
 * keys/one-time tokens should not be conflated with the durable record ID
 * they authorize).
 */
export const EnrollmentChallengeIdSchema = idSchema('EnrollmentChallengeId');
export type EnrollmentChallengeId = z.infer<typeof EnrollmentChallengeIdSchema>;

/** `Docs/SPEC.md` 3.12 Build execution and provenance. */
export const BuildIdSchema = idSchema('BuildId');
export type BuildId = z.infer<typeof BuildIdSchema>;

/**
 * `Docs/SPEC.md` 3.12 "job ID" and Section 4.7 Durable job — the pervasive
 * durable-job identifier referenced by `OperationResult`, MCP tool results,
 * and every long-running device/build operation.
 */
export const JobIdSchema = idSchema('JobId');
export type JobId = z.infer<typeof JobIdSchema>;

/** `Docs/SPEC.md` 3.13 Notification. */
export const NotificationIdSchema = idSchema('NotificationId');
export type NotificationId = z.infer<typeof NotificationIdSchema>;

/** `Docs/SPEC.md` 3.13 Work intent. */
export const WorkIntentIdSchema = idSchema('WorkIntentId');
export type WorkIntentId = z.infer<typeof WorkIntentIdSchema>;

// --- Cross-cutting envelope identifiers -------------------------------------------

/** `Docs/SPEC.md` Section 1 `Envelope<T>.message_id`. */
export const MessageIdSchema = idSchema('MessageId');
export type MessageId = z.infer<typeof MessageIdSchema>;

/**
 * `Docs/SPEC.md` Section 1 `Envelope<T>.correlation_id` — required on every
 * cross-process envelope, error, and MCP result per AGENTS.md rule 5.
 */
export const CorrelationIdSchema = idSchema('CorrelationId');
export type CorrelationId = z.infer<typeof CorrelationIdSchema>;

/** `Docs/SPEC.md` Section 1 `Envelope<T>.causation_id?`. */
export const CausationIdSchema = idSchema('CausationId');
export type CausationId = z.infer<typeof CausationIdSchema>;

// --- Content digests ----------------------------------------------------------------

/**
 * `Docs/SPEC.md` 3.8 Artifact "immutable digest". The exact hash algorithm is
 * a producer decision outside this package's scope; the shared shape only
 * bounds length/charset (see `./brand.ts`).
 */
export const ArtifactDigestSchema = digestSchema('ArtifactDigest');
export type ArtifactDigest = z.infer<typeof ArtifactDigestSchema>;

/**
 * `Docs/SPEC.md` Section 1 `OperationResult.approval_required.bound_request_digest`
 * and `Docs/MCP_SPEC.md` Section 7 — the canonicalized-input digest an
 * approval is bound to. `Docs/SPEC.md` AS-007 requires any change to this
 * digest's bound inputs to invalidate the approval.
 */
export const BoundRequestDigestSchema = digestSchema('BoundRequestDigest');
export type BoundRequestDigest = z.infer<typeof BoundRequestDigestSchema>;

/** `Docs/CONNECTOR_SPEC.md` Section 9 "Pin and verify exact compatible versions." */
export const ToolchainDigestSchema = digestSchema('ToolchainDigest');
export type ToolchainDigest = z.infer<typeof ToolchainDigestSchema>;

/** `Docs/SPEC.md` 3.12 Build execution "quarantined outputs" content digest. */
export const OutputDigestSchema = digestSchema('OutputDigest');
export type OutputDigest = z.infer<typeof OutputDigestSchema>;
