import { z } from 'zod';
import { CorrelationIdSchema } from './ids.js';
import {
  AuthenticatedPrincipalSchema,
  BUSINESS_RPC_CAPABILITY,
  BusinessEventSchema,
  BusinessRpcRequestSchema,
  BusinessRpcResultSchema,
} from './businessRpc.js';

/**
 * Envelope-level wire protocol contract — `P1-NODE-IPC-CLIENT-001`.
 *
 * `./businessRpc.ts` covers only business method/result *payload* shapes
 * (confirmed by direct inspection: it imports/defines nothing named
 * `WireMessage`/`CallOutcome`/`HandshakeRejectReason`). This module is the
 * first envelope-level TypeScript contract in this package, mirroring
 * `crates/loki-ipc/src/protocol.rs` field-for-field and tag-for-tag —
 * including every non-`snake_case` tag value Rust's `#[serde(...)]`
 * attributes actually produce (several enums here deliberately do **not**
 * use `rename_all = "snake_case"`; see each schema's own comment for the
 * exact Rust attribute it mirrors). Every object schema is `.strict()`
 * where the Rust side declares `#[serde(deny_unknown_fields)]`, or where
 * this module chooses the same posture for a type not itself so annotated
 * but only ever produced by a trusted peer.
 *
 * This module never invents a shape independently of `protocol.rs` — every
 * schema below cites the exact Rust type/attribute it mirrors.
 */

/** Mirrors `loki_ipc::protocol::ProtocolVersion` (plain struct, no rename_all). */
export const ProtocolVersionSchema = z
  .object({
    major: z.number().int().min(0).max(65_535),
    minor: z.number().int().min(0).max(65_535),
  })
  .strict();
export type ProtocolVersion = z.infer<typeof ProtocolVersionSchema>;

/** Mirrors `loki_ipc::protocol::PROTOCOL_VERSION` (`ProtocolVersion::new(1, 1)`). */
export const PROTOCOL_VERSION: ProtocolVersion = { major: 1, minor: 1 };

/** Mirrors `loki_ipc::protocol::CORE_CAPABILITY`. */
export const CORE_CAPABILITY = 'loki-ipc.core.v1' as const;

/** Mirrors `loki_ipc::business::BUSINESS_CAPABILITY` — re-exported here
 * under its envelope-relevant name for a caller that only imports this
 * module. Same string `businessRpc.ts` already exports as
 * `BUSINESS_RPC_CAPABILITY` — not a second, independently-defined value. */
export const BUSINESS_CAPABILITY = BUSINESS_RPC_CAPABILITY;

/**
 * A safe-id-shaped UUID-carrying field (`correlation_id`/`session_id`/
 * `nonce`/`server_run_id`). Rust types these `uuid::Uuid`, which serializes
 * as a canonical hyphenated hex string — a strict subset of the
 * already-established `CorrelationIdSchema` safe-id bound this package uses
 * for every other opaque identifier (`./ids.ts`), so this reuses it rather
 * than inventing a second, parallel UUID-specific brand.
 */
const WireUuidSchema = CorrelationIdSchema;

/** Mirrors `loki_ipc::protocol::PingParams`. No explicit Rust-side length
 * bound beyond the frame codec's own `DEFAULT_MAX_FRAME_BYTES`. */
const PingParamsSchema = z.object({ payload: z.string() }).strict();

/** Mirrors `loki_ipc::protocol::PongResult`. */
export const PongResultSchema = z
  .object({
    payload: z.string(),
    server_unix_millis: z.number().int().nonnegative(),
  })
  .strict();

/**
 * Mirrors `loki_ipc::protocol::Method` exactly: `#[serde(tag = "method",
 * content = "params", rename_all = "snake_case", deny_unknown_fields)]`,
 * with `Ping` overridden via `#[serde(rename = "Ping")]` (the one variant
 * that keeps its Rust-identifier casing instead of `rename_all`'s
 * `snake_case`). The other 23 variants are exactly
 * `BusinessRpcRequestSchema` (`./businessRpc.ts`) — same tag/content field
 * names, same snake_case literals — reused here rather than redefined, per
 * this node's "no duplicate/parallel TS type definitions for the same
 * payloads" requirement.
 */
export const MethodSchema = z.union([
  z.object({ method: z.literal('Ping'), params: PingParamsSchema }).strict(),
  BusinessRpcRequestSchema,
]);
export type Method = z.infer<typeof MethodSchema>;

/** Mirrors `loki_ipc::protocol::EchoStreamParams`. */
const EchoStreamParamsSchema = z
  .object({ payload: z.string(), count: z.number().int().nonnegative() })
  .strict();

/** Mirrors `loki_ipc::protocol::MAX_ECHO_STREAM_COUNT`. */
export const MAX_ECHO_STREAM_COUNT = 10_000;

/**
 * Mirrors `loki_ipc::business::BusinessStreamParams` (the wire shape
 * `StreamMethod::BusinessEvents` carries) — not otherwise exported from
 * `businessRpc.ts` (that module exports `BusinessStreamRequestSchema` for
 * the same fields under a different name/context; reused here rather than
 * redefined).
 */
const BusinessEventsParamsSchema = z
  .object({
    stream: z.enum([
      'runtime_status',
      'device_projection',
      'device_health',
      'telemetry',
      'logs',
      'incidents',
      'jobs',
      'approvals',
      'notifications',
      'work_intents',
    ]),
    entity_id: z.string().min(1).max(200),
    after_sequence: z.number().int().nonnegative().nullable().optional(),
    max_items: z.number().int().min(1).max(1_024),
  })
  .strict();

/**
 * Mirrors `loki_ipc::protocol::StreamMethod`: `#[serde(tag = "method",
 * content = "params", rename_all = "snake_case", deny_unknown_fields)]`,
 * `EchoStream` overridden via `#[serde(rename = "EchoStream")]`;
 * `BusinessEvents` stays `rename_all`'s default -> `"business_events"`.
 */
export const StreamMethodSchema = z.union([
  z.object({ method: z.literal('EchoStream'), params: EchoStreamParamsSchema }).strict(),
  z.object({ method: z.literal('business_events'), params: BusinessEventsParamsSchema }).strict(),
]);
export type StreamMethod = z.infer<typeof StreamMethodSchema>;

/** Mirrors `loki_ipc::protocol::EchoStreamItem`. */
const EchoStreamItemSchema = z
  .object({ seq: z.number().int().nonnegative(), payload: z.string() })
  .strict();

/** Mirrors `loki_ipc::protocol::EchoStreamSummary`. */
const EchoStreamSummarySchema = z.object({ items_sent: z.number().int().nonnegative() }).strict();

/** Mirrors `loki_ipc::business::BusinessStreamSummary`. */
export const BusinessStreamSummarySchema = z
  .object({
    items_sent: z.number().int().min(0).max(65_535),
    last_sequence: z.number().int().nonnegative().nullable().optional(),
  })
  .strict();

/**
 * Mirrors `loki_ipc::protocol::MethodResult`: `#[serde(tag = "method",
 * content = "result")]` with **no** `rename_all` — the tag values are the
 * bare Rust variant identifiers, `"Ping"`/`"Business"` (not snake_case).
 */
export const MethodResultSchema = z.union([
  z.object({ method: z.literal('Ping'), result: PongResultSchema }).strict(),
  z.object({ method: z.literal('Business'), result: BusinessRpcResultSchema }).strict(),
]);
export type MethodResult = z.infer<typeof MethodResultSchema>;

/**
 * Mirrors `loki_ipc::protocol::StreamItemPayload`: `#[serde(tag = "method",
 * content = "item")]`, no `rename_all` -> `"EchoStream"`/`"BusinessEvent"`.
 */
export const StreamItemPayloadSchema = z.union([
  z.object({ method: z.literal('EchoStream'), item: EchoStreamItemSchema }).strict(),
  z.object({ method: z.literal('BusinessEvent'), item: BusinessEventSchema }).strict(),
]);
export type StreamItemPayload = z.infer<typeof StreamItemPayloadSchema>;

/**
 * Mirrors `loki_ipc::protocol::StreamSummary`: `#[serde(tag = "method",
 * content = "summary")]`, no `rename_all` -> `"EchoStream"`/`"BusinessEvents"`.
 */
export const StreamSummarySchema = z.union([
  z.object({ method: z.literal('EchoStream'), summary: EchoStreamSummarySchema }).strict(),
  z.object({ method: z.literal('BusinessEvents'), summary: BusinessStreamSummarySchema }).strict(),
]);
export type StreamSummary = z.infer<typeof StreamSummarySchema>;

/** Mirrors `loki_ipc::protocol::RpcError`: `#[serde(tag = "kind", rename_all = "snake_case")]`. */
export const RpcErrorSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('deadline_exceeded') }).strict(),
  z.object({ kind: z.literal('cancelled') }).strict(),
  z.object({ kind: z.literal('invalid_request'), message: z.string() }).strict(),
  z.object({ kind: z.literal('missing_capability'), required: z.string() }).strict(),
  z.object({ kind: z.literal('permission_denied') }).strict(),
  z.object({ kind: z.literal('credential_revalidation_failed') }).strict(),
  z.object({ kind: z.literal('unsupported_method'), method: z.string() }).strict(),
  z.object({ kind: z.literal('internal'), message: z.string() }).strict(),
]);
export type RpcError = z.infer<typeof RpcErrorSchema>;

/** Mirrors `loki_ipc::protocol::CallOutcome`: `#[serde(tag = "outcome", rename_all = "snake_case")]`. */
export const CallOutcomeSchema = z.discriminatedUnion('outcome', [
  z.object({ outcome: z.literal('ok'), result: MethodResultSchema }).strict(),
  z.object({ outcome: z.literal('err'), error: RpcErrorSchema }).strict(),
]);
export type CallOutcome = z.infer<typeof CallOutcomeSchema>;

/** Mirrors `loki_ipc::protocol::StreamOutcome`: `#[serde(tag = "outcome", rename_all = "snake_case")]`. */
export const StreamOutcomeSchema = z.discriminatedUnion('outcome', [
  z.object({ outcome: z.literal('ok'), summary: StreamSummarySchema }).strict(),
  z.object({ outcome: z.literal('err'), error: RpcErrorSchema }).strict(),
]);
export type StreamOutcome = z.infer<typeof StreamOutcomeSchema>;

/** Mirrors `loki_ipc::protocol::HandshakeRejectReason`: `#[serde(tag = "reason", rename_all = "snake_case")]`. */
export const HandshakeRejectReasonSchema = z.discriminatedUnion('reason', [
  z
    .object({
      reason: z.literal('version_mismatch'),
      server_version: ProtocolVersionSchema,
      client_version: ProtocolVersionSchema,
    })
    .strict(),
  z.object({ reason: z.literal('missing_capability'), required: z.string() }).strict(),
  z.object({ reason: z.literal('authentication_failed') }).strict(),
  z.object({ reason: z.literal('stale_nonce') }).strict(),
  z.object({ reason: z.literal('malformed_hello'), message: z.string() }).strict(),
]);
export type HandshakeRejectReason = z.infer<typeof HandshakeRejectReasonSchema>;

/**
 * Mirrors `loki_ipc::protocol::WireMessage`: `#[serde(tag = "type")]`, no
 * `rename_all` — every tag value is the bare Rust variant identifier
 * (`"Challenge"`, `"Hello"`, ... `"CancelCall"`), not snake_case. This is
 * the exact frame payload shape `packages/ipc-client`'s codec
 * serializes/parses — see that package's own `frame.ts`.
 */
export const WireMessageSchema = z.union([
  z
    .object({
      type: z.literal('Challenge'),
      server_protocol_version: ProtocolVersionSchema,
      server_capabilities: z.array(z.string()),
      server_run_id: WireUuidSchema,
      nonce: WireUuidSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal('Hello'),
      client_protocol_version: ProtocolVersionSchema,
      client_capabilities: z.array(z.string()),
      client_type: z.string(),
      client_version: z.string(),
      nonce: WireUuidSchema,
      token: z.string(),
    })
    .strict(),
  z
    .object({
      type: z.literal('HelloAck'),
      negotiated_capabilities: z.array(z.string()),
      session_id: WireUuidSchema,
      principal: AuthenticatedPrincipalSchema.nullable().optional(),
    })
    .strict(),
  z
    .object({
      type: z.literal('HelloReject'),
      reason: HandshakeRejectReasonSchema,
      server_protocol_version: ProtocolVersionSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal('Call'),
      correlation_id: WireUuidSchema,
      deadline_unix_millis: z.number().int().nonnegative(),
      method: MethodSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal('CallResult'),
      correlation_id: WireUuidSchema,
      outcome: CallOutcomeSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal('CallStream'),
      correlation_id: WireUuidSchema,
      deadline_unix_millis: z.number().int().nonnegative(),
      method: StreamMethodSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal('StreamItem'),
      correlation_id: WireUuidSchema,
      seq: z.number().int().nonnegative(),
      item: StreamItemPayloadSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal('StreamEnd'),
      correlation_id: WireUuidSchema,
      outcome: StreamOutcomeSchema,
    })
    .strict(),
  z.object({ type: z.literal('CancelCall'), correlation_id: WireUuidSchema }).strict(),
]);
export type WireMessage = z.infer<typeof WireMessageSchema>;
