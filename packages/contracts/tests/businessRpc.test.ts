import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  AuthenticatedPrincipalSchema,
  BusinessEventSchema,
  BusinessRpcRequestSchema,
  BusinessRpcResultSchema,
  BusinessStreamRequestSchema,
  businessResultMatchesRequest,
} from '../src/businessRpc.js';

const fixture = JSON.parse(
  readFileSync(new URL('../fixtures/business-rpc.json', import.meta.url), 'utf8'),
) as {
  requests: unknown[];
  results: unknown[];
  streams: unknown[];
  events: unknown[];
  principals: unknown[];
  invalid_results: unknown[];
  binding_negatives: Array<{ method: string; field: string; value: unknown }>;
};

describe('canonical business RPC', () => {
  it('accepts explicit bounded methods', () => {
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'device_get_health',
        params: { device_id: 'dev-1', limit: 20 },
      }).success,
    ).toBe(true);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'flash_request',
        params: {
          device_id: 'dev-1',
          artifact_id: 'art-1',
          artifact_digest: '0123456789abcdef0123456789abcdef',
          idempotency_key: 'idem-1',
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
        params: { device_id: 'dev-1', idempotency_key: 'idem-1', authority: ['approval_grant'] },
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
          idempotency_key: 'idem-1',
          path: 'C:\\',
        },
      }).success,
    ).toBe(false);
    expect(
      BusinessStreamRequestSchema.safeParse({
        stream: 'telemetry',
        workspace_id: 'workspace-1',
        scope_id: 'device-1',
        max_items: 1_025,
      }).success,
    ).toBe(false);
    expect(
      BusinessRpcRequestSchema.safeParse({
        method: 'restart_request',
        params: { device_id: 'dev/escape', idempotency_key: 'idem-1' },
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
          idempotency_key: 'idem-1',
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
          idempotency_key: 'idem-1',
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
        params: { limit: 10, cursor: null },
      }).success,
    ).toBe(true);
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'job_get',
        meta: {
          source_runtime_id: 'runtime-1',
          observed_at: '2026-08-24T00:00:00.000Z',
          correlation_id: 'corr-1',
        },
        job: {
          job_id: 'job-1',
          state: 'failed',
          updated_at: '2026-08-24T00:00:00.000Z',
          error_code: null,
        },
      }).success,
    ).toBe(true);
  });

  it('rejects invented principal authority values', () => {
    const helloPrincipal = {
      kind: 'agent_connection',
      principal_id: 'agent-1',
      workspace_id: 'workspace-1',
      scope_ids: ['device-1'],
      authority: ['root'],
    };
    expect(AuthenticatedPrincipalSchema.safeParse(helloPrincipal).success).toBe(false);
  });

  it('enforces UTF-8 byte limits and UTC timestamps on outputs', () => {
    const oversizedUtf8 = '😀'.repeat(501);
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'operation',
        method: 'restart_request',
        outcome: 'rejected',
        correlation_id: 'corr-1',
        policy_reason: oversizedUtf8,
        idempotency_key: 'idem-1',
        artifact_digest: null,
        request_reason: null,
        scope_ids: ['device:dev-1'],
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

  it('validates bounded structured results', () => {
    expect(
      BusinessRpcResultSchema.safeParse({
        kind: 'operation',
        method: 'restart_request',
        outcome: 'accepted',
        correlation_id: 'corr-1',
        policy_reason: 'policy allows request',
        idempotency_key: 'idem-1',
        artifact_digest: null,
        request_reason: null,
        job_id: 'job-1',
        scope_ids: ['device:dev-1'],
      }).success,
    ).toBe(true);
  });

  it('accepts the complete Rust/TypeScript compatibility fixture', () => {
    expect(
      fixture.requests.every((value) => BusinessRpcRequestSchema.safeParse(value).success),
    ).toBe(true);
    expect(fixture.results.every((value) => BusinessRpcResultSchema.safeParse(value).success)).toBe(
      true,
    );
    expect(
      fixture.streams.every((value) => BusinessStreamRequestSchema.safeParse(value).success),
    ).toBe(true);
    expect(fixture.events.every((value) => BusinessEventSchema.safeParse(value).success)).toBe(
      true,
    );
    expect(
      fixture.principals.every((value) => AuthenticatedPrincipalSchema.safeParse(value).success),
    ).toBe(true);
    expect(
      fixture.requests.every((request) =>
        fixture.results.some((result) =>
          businessResultMatchesRequest(request, result, 'workspace-1'),
        ),
      ),
    ).toBe(true);
  });

  it('rejects unknown fields on every result variant and nested projection', () => {
    for (const result of fixture.results) {
      expect(
        BusinessRpcResultSchema.safeParse({
          ...(result as Record<string, unknown>),
          unexpected: true,
        }).success,
      ).toBe(false);
    }
    const devices = structuredClone(fixture.results[1]) as {
      meta: Record<string, unknown>;
      devices: Record<string, unknown>[];
    };
    devices.meta.unexpected = true;
    expect(BusinessRpcResultSchema.safeParse(devices).success).toBe(false);
    delete devices.meta.unexpected;
    devices.devices.push({
      device_id: 'dev-1',
      lifecycle_state: 'ready',
      connector_id: 'connector-1',
      last_seen_at: '2026-08-24T12:00:00Z',
    });
    devices.devices[0]!.unexpected = true;
    expect(BusinessRpcResultSchema.safeParse(devices).success).toBe(false);
    expect(
      fixture.invalid_results.every((value) => !BusinessRpcResultSchema.safeParse(value).success),
    ).toBe(true);
  });

  it('rejects shared changed-request and approval-digest binding negatives', () => {
    for (const negative of fixture.binding_negatives) {
      const request = fixture.requests.find(
        (candidate) => (candidate as { method?: unknown }).method === negative.method,
      );
      const result = structuredClone(
        fixture.results.find(
          (candidate) =>
            (candidate as { kind?: unknown; method?: unknown }).kind === 'operation' &&
            (candidate as { method?: unknown }).method === negative.method,
        ),
      ) as Record<string, unknown>;
      expect(request).toBeDefined();
      expect(result).toBeDefined();
      result[negative.field] = negative.value;
      expect(BusinessRpcResultSchema.safeParse(result).success).toBe(true);
      expect(businessResultMatchesRequest(request, result, 'workspace-1')).toBe(false);
    }
  });
});
