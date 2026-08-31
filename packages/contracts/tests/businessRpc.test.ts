import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  AuthenticatedPrincipalSchema,
  BusinessEventSchema,
  BusinessRpcRequestSchema,
  BusinessRpcResultSchema,
  BusinessStreamRequestSchema,
} from '../src/businessRpc.js';

const fixture = JSON.parse(
  readFileSync(new URL('../fixtures/business-rpc.json', import.meta.url), 'utf8'),
) as {
  requests: unknown[];
  results: unknown[];
  streams: unknown[];
  events: unknown[];
  principals: unknown[];
  negative: Array<{ description: string; kind: string; value: unknown }>;
};

const schemaFor = (kind: string) => {
  switch (kind) {
    case 'request':
      return BusinessRpcRequestSchema;
    case 'result':
      return BusinessRpcResultSchema;
    case 'stream':
      return BusinessStreamRequestSchema;
    case 'event':
      return BusinessEventSchema;
    case 'principal':
      return AuthenticatedPrincipalSchema;
    default:
      throw new Error(`unknown negative fixture kind: ${kind}`);
  }
};

describe('canonical business RPC', () => {
  it('accepts explicit bounded methods', () => {
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'device_get_health',
        params: { device_id: 'dev-1' },
      }).success,
    ).toBe(true);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'flash_request',
        params: {
          device_id: 'dev-1',
          artifact_id: 'art-1',
          artifact_digest: '0123456789abcdef0123456789abcdef',
          parameters_digest: 'params-digest-fixture-06',
          idempotency_key: 'idem-1',
          approval_reference: null,
        },
      }).success,
    ).toBe(true);
  });

  it('rejects generic invocation and authority supplied by a client', () => {
    expect(
      BusinessRpcRequestSchema.safeParse({ method: 'invoke', params: { command: 'flash' } })
        .success,
    ).toBe(false);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'restart_request',
        params: {
          device_id: 'dev-1',
          parameters_digest: 'params-digest-fixture-05',
          idempotency_key: 'idem-1',
          approval_reference: null,
          authority: ['approval_grant'],
        },
      }).success,
    ).toBe(false);
  });

  it('rejects unrestricted execution and unbounded stream inputs', () => {
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'build_request',
        params: {
          source_snapshot_id: 'snap-1',
          target_id: 'target-1',
          recipe_id: 'recipe-1',
          parameters_digest: 'params-digest-fixture-04',
          idempotency_key: 'idem-1',
          approval_reference: null,
          path: 'C:\\',
        },
      }).success,
    ).toBe(false);
    expect(
      BusinessStreamRequestSchema.safeParse({
        stream: 'telemetry',
        entity_id: 'device-1',
        max_items: 1_025,
      }).success,
    ).toBe(false);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'restart_request',
        params: {
          device_id: 'dev/escape',
          parameters_digest: 'params-digest-fixture-05',
          idempotency_key: 'idem-1',
          approval_reference: null,
        },
      }).success,
    ).toBe(false);
  });

  it('never accepts a workspace_id/tenant_id field anywhere (ADR-0006 Amendment 1)', () => {
    expect(
      BusinessStreamRequestSchema.safeParse({
        stream: 'telemetry',
        workspace_id: 'workspace-1',
        entity_id: 'device-1',
        max_items: 10,
      }).success,
    ).toBe(false);
    expect(
      AuthenticatedPrincipalSchema.safeParse({
        kind: 'agent_connection',
        principal_id: 'agent-1',
        workspace_id: 'workspace-1',
        authorized_entity_ids: ['device-1'],
        authority: ['read_devices'],
      }).success,
    ).toBe(false);
  });

  it('keeps identifier and digest character rules identical at both language boundaries', () => {
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'build_request',
        params: {
          source_snapshot_id: 'snap/escape',
          target_id: 'target-1',
          recipe_id: 'recipe-1',
          parameters_digest: 'params-digest-fixture-04',
          idempotency_key: 'idem-1',
          approval_reference: null,
        },
      }).success,
    ).toBe(false);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'test_on_device_request',
        params: { device_id: 'dev-1', test_suite_id: 'suite/escape', idempotency_key: 'idem-1' },
      }).success,
    ).toBe(false);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'flash_request',
        params: {
          device_id: 'dev-1',
          artifact_id: 'art-1',
          artifact_digest: 'not+base64url++++',
          parameters_digest: 'params-digest-fixture-06',
          idempotency_key: 'idem-1',
          approval_reference: null,
        },
      }).success,
    ).toBe(false);
  });

  it('defines omission and explicit null equivalently for nullable optional fields', () => {
    expect(
      BusinessRpcRequestSchema.safeParse({ method: 'devices_list', params: { limit: 10 } }).success,
    ).toBe(true);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'devices_list',
        params: { limit: 10, after_device_id: null },
      }).success,
    ).toBe(true);
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'device_get',
        meta: {
          source_runtime_id: 'runtime-1',
          observed_at: '2026-08-24T00:00:00.000Z',
          correlation_id: 'corr-1',
        },
        device: null,
      }).success,
    ).toBe(true);
  });

  it('rejects invented principal authority values', () => {
    const helloPrincipal = {
      kind: 'agent_connection',
      principal_id: 'agent-1',
      authorized_entity_ids: ['device-1'],
      authority: ['root'],
    };
    expect(AuthenticatedPrincipalSchema.safeParse(helloPrincipal).success).toBe(false);
  });

  it('enforces UTF-8 byte limits and UTC timestamps on outputs', () => {
    const oversizedUtf8 = '😀'.repeat(501);
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'mutation',
        method: 'restart',
        correlation_id: 'corr-1',
        target_id: 'dev-1',
        outcome: {
          outcome: 'rejected',
          rejection: { kind: 'grant_invalid', reason: oversizedUtf8 },
        },
      }).success,
    ).toBe(false);
    expect(
      BusinessEventSchema.safeParse({
        stream: 'runtime_status',
        sequence: 1,
        correlation_id: 'corr-1',
        occurred_at: '2026-08-24T12:00:00+05:30',
        entity_id: 'runtime-1',
        state: 'connected',
        source_runtime_id: 'runtime-1',
      }).success,
    ).toBe(false);
  });

  it('validates bounded structured mutation results, discriminated by real AcceptanceResult shape', () => {
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'mutation',
        method: 'restart',
        correlation_id: 'corr-1',
        target_id: 'dev-1',
        outcome: {
          outcome: 'accepted',
          job_id: 'job-1',
          correlation_id: 'corr-1',
          accepted_at_ms: 1,
        },
      }).success,
    ).toBe(true);
    // A contradictory/collapsed shape (fields from two different outcomes at
    // once) must never validate — proves the discriminated union, not a flat
    // struct with optional fields, is what's on the wire.
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'mutation',
        method: 'restart',
        correlation_id: 'corr-1',
        target_id: 'dev-1',
        outcome: {
          outcome: 'accepted',
          job_id: 'job-1',
          correlation_id: 'corr-1',
          accepted_at_ms: 1,
          approval_id: 'apr-1',
        },
      }).success,
    ).toBe(false);
  });

  it('accepts the complete Rust/TypeScript compatibility fixture', () => {
    for (const value of fixture.requests) {
      const result = BusinessRpcRequestSchema.safeParse(value);
      expect(result.success, JSON.stringify({ value, error: result.error?.message })).toBe(true);
    }
    for (const value of fixture.results) {
      const result = BusinessRpcResultSchema.safeParse(value);
      expect(result.success, JSON.stringify({ value, error: result.error?.message })).toBe(true);
    }
    for (const value of fixture.streams) {
      const result = BusinessStreamRequestSchema.safeParse(value);
      expect(result.success, JSON.stringify({ value, error: result.error?.message })).toBe(true);
    }
    for (const value of fixture.events) {
      const result = BusinessEventSchema.safeParse(value);
      expect(result.success, JSON.stringify({ value, error: result.error?.message })).toBe(true);
    }
    for (const value of fixture.principals) {
      const result = AuthenticatedPrincipalSchema.safeParse(value);
      expect(result.success, JSON.stringify({ value, error: result.error?.message })).toBe(true);
    }
    expect(fixture.requests.length).toBe(23);
    expect(fixture.streams.length).toBe(10);
  });

  it('rejects every entry in the Rust-shared negative fixture set', () => {
    expect(fixture.negative.length).toBeGreaterThanOrEqual(8);
    for (const negative of fixture.negative) {
      const schema = schemaFor(negative.kind);
      const result = schema.safeParse(negative.value);
      expect(result.success, `expected rejection: ${negative.description}`).toBe(false);
    }
  });
});
