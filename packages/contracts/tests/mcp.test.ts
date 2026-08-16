import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  CursorSchema,
  KNOWN_MCP_TOOL_NAMES,
  KnownMcpToolNameSchema,
  MCP_OPERATION_OUTCOMES,
  MCP_TOOL_CATEGORY,
  McpOperationOutcomeSchema,
  McpOperationResultSchema,
  McpToolNameSchema,
  mcpReadResultSchema,
  parseLokiResourceUri,
} from '../src/mcp.js';
import { expectInvalid, expectValid, omit } from './helpers.js';

describe('CursorSchema', () => {
  it('accepts an opaque cursor token', () => {
    expectValid(CursorSchema, 'opaque-cursor-token-1');
  });

  it('rejects an empty cursor', () => {
    expectInvalid(CursorSchema, '');
  });
});

describe('mcpReadResultSchema', () => {
  const dataSchema = z.object({ device_id: z.string() });
  const schema = mcpReadResultSchema(dataSchema);

  const valid = {
    data: { device_id: 'device-1' },
    source: 'runtime-cache',
    observed_at: '2026-08-15T12:00:00Z',
    freshness: 'current',
    correlation_id: 'corr-1',
  };

  it('accepts a well-formed read result without a cursor', () => {
    expectValid(schema, valid);
  });

  it('accepts a well-formed read result with next_cursor', () => {
    expectValid(schema, { ...valid, next_cursor: 'cursor-2' });
  });

  it('rejects a missing correlation_id', () => {
    expectInvalid(schema, omit(valid, 'correlation_id'));
  });

  it('rejects data that fails the caller-supplied data schema', () => {
    expectInvalid(schema, { ...valid, data: { device_id: 42 } });
  });

  it('rejects a non-UTC observed_at', () => {
    expectInvalid(schema, { ...valid, observed_at: '2026-08-15T12:00:00' });
  });
});

describe('McpOperationOutcomeSchema', () => {
  it('accepts every documented outcome value', () => {
    for (const outcome of MCP_OPERATION_OUTCOMES) {
      expectValid(McpOperationOutcomeSchema, outcome);
    }
  });

  it('rejects an unknown outcome value', () => {
    expectInvalid(McpOperationOutcomeSchema, 'queued');
  });

  it('rejects a missing value', () => {
    expectInvalid(McpOperationOutcomeSchema, undefined);
  });
});

describe('McpOperationResultSchema', () => {
  const valid = {
    outcome: 'accepted',
    policy_reason: 'workspace policy permits restart without approval',
    correlation_id: 'corr-1',
  };

  it('accepts every documented outcome value', () => {
    for (const outcome of MCP_OPERATION_OUTCOMES) {
      expectValid(McpOperationResultSchema, { ...valid, outcome });
    }
  });

  it('accepts the full optional field set', () => {
    expectValid(McpOperationResultSchema, {
      outcome: 'approval_required',
      approval_id: 'approval-1',
      job_id: 'job-1',
      bound_request_digest: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85',
      policy_reason: 'flash requires approval',
      correlation_id: 'corr-1',
      status_resource: 'loki://jobs/job-1',
    });
  });

  it('rejects an unknown outcome value', () => {
    expectInvalid(McpOperationResultSchema, { ...valid, outcome: 'queued' });
  });

  it('rejects a missing policy_reason', () => {
    expectInvalid(McpOperationResultSchema, omit(valid, 'policy_reason'));
  });

  it('rejects a malformed optional bound_request_digest', () => {
    expectInvalid(McpOperationResultSchema, { ...valid, bound_request_digest: 'too-short' });
  });
});

describe('McpToolNameSchema', () => {
  it('accepts every known proposed tool name', () => {
    for (const name of KNOWN_MCP_TOOL_NAMES) {
      expectValid(McpToolNameSchema, name);
    }
  });

  it('accepts a well-formed but unlisted loki_ tool name (future MCP-001 addition)', () => {
    expectValid(McpToolNameSchema, 'loki_future_tool');
  });

  it('rejects a name missing the loki_ prefix', () => {
    expectInvalid(McpToolNameSchema, 'list_devices');
  });

  it('rejects a name with disallowed casing', () => {
    expectInvalid(McpToolNameSchema, 'loki_ListDevices');
  });

  it('rejects an oversized tool name', () => {
    expectInvalid(McpToolNameSchema, `loki_${'a'.repeat(64)}`);
  });
});

describe('KnownMcpToolNameSchema and MCP_TOOL_CATEGORY', () => {
  it('accepts every name MCP_TOOL_CATEGORY classifies', () => {
    for (const name of Object.keys(MCP_TOOL_CATEGORY)) {
      expectValid(KnownMcpToolNameSchema, name);
    }
  });

  it('rejects a tool name outside the accepted v1.2 proposed list', () => {
    expectInvalid(KnownMcpToolNameSchema, 'loki_delete_everything');
  });

  it('classifies exactly the ten inspection-tier tools from Docs/MCP_SPEC.md Section 6', () => {
    const inspectionTools = Object.entries(MCP_TOOL_CATEGORY)
      .filter(([, category]) => category === 'inspection')
      .map(([name]) => name)
      .sort();
    expect(inspectionTools).toEqual(
      [
        'loki_list_devices',
        'loki_get_device',
        'loki_get_device_health',
        'loki_read_device_logs',
        'loki_query_telemetry',
        'loki_list_incidents',
        'loki_get_incident',
        'loki_get_job',
        'loki_list_deployments',
        'loki_get_deployment',
      ].sort(),
    );
  });

  it('classifies exactly the five consequential-tier tools from Docs/MCP_SPEC.md Section 6', () => {
    const consequentialTools = Object.entries(MCP_TOOL_CATEGORY)
      .filter(([, category]) => category === 'consequential')
      .map(([name]) => name)
      .sort();
    expect(consequentialTools).toEqual(
      [
        'loki_request_restart',
        'loki_request_flash',
        'loki_request_test_on_device',
        'loki_request_deployment',
        'loki_request_rollback',
      ].sort(),
    );
  });
});

describe('parseLokiResourceUri', () => {
  it('parses every documented resource URI family with a valid id', () => {
    expect(parseLokiResourceUri('loki://workspaces/workspace-1/summary')).toEqual({
      family: 'workspace_summary',
      workspace_id: 'workspace-1',
    });
    expect(parseLokiResourceUri('loki://devices/device-1')).toEqual({
      family: 'device',
      device_id: 'device-1',
    });
    expect(parseLokiResourceUri('loki://devices/device-1/logs')).toEqual({
      family: 'device_logs',
      device_id: 'device-1',
    });
    expect(parseLokiResourceUri('loki://devices/device-1/telemetry')).toEqual({
      family: 'device_telemetry',
      device_id: 'device-1',
    });
    expect(parseLokiResourceUri('loki://incidents/incident-1')).toEqual({
      family: 'incident',
      incident_id: 'incident-1',
    });
    expect(parseLokiResourceUri('loki://incidents/incident-1/evidence')).toEqual({
      family: 'incident_evidence',
      incident_id: 'incident-1',
    });
    expect(parseLokiResourceUri('loki://deployments/deployment-1')).toEqual({
      family: 'deployment',
      deployment_id: 'deployment-1',
    });
    expect(parseLokiResourceUri('loki://jobs/job-1')).toEqual({ family: 'job', job_id: 'job-1' });
    expect(parseLokiResourceUri('loki://agents/runs/run-1')).toEqual({
      family: 'agent_run',
      agent_run_id: 'run-1',
    });
  });

  it('returns null for an unsupported scheme', () => {
    expect(parseLokiResourceUri('http://devices/device-1')).toBeNull();
  });

  it('returns null for an unknown resource family', () => {
    expect(parseLokiResourceUri('loki://unknown-family/1')).toBeNull();
  });

  it('returns null for a missing id segment', () => {
    expect(parseLokiResourceUri('loki://devices/')).toBeNull();
    expect(parseLokiResourceUri('loki://devices')).toBeNull();
  });

  it('returns null when the id segment structurally matches but fails ID validation', () => {
    // Single path segment (no extra slashes, so the URI shape itself matches
    // the `device` family pattern) containing a character DeviceIdSchema
    // disallows — exercises the "matched, then ID validation rejected it"
    // branch specifically, distinct from an unmatched URI shape.
    expect(parseLokiResourceUri('loki://devices/inva!id')).toBeNull();
  });

  it('returns null for a path-traversal-shaped id (fails URI shape matching)', () => {
    expect(parseLokiResourceUri('loki://devices/../../etc/passwd')).toBeNull();
  });

  it('returns null for a non-string input', () => {
    expect(parseLokiResourceUri(undefined)).toBeNull();
    expect(parseLokiResourceUri(42)).toBeNull();
    expect(parseLokiResourceUri({ uri: 'loki://devices/device-1' })).toBeNull();
  });

  it('returns null for trailing garbage after a valid resource path', () => {
    expect(parseLokiResourceUri('loki://devices/device-1/logs/extra')).toBeNull();
  });
});
