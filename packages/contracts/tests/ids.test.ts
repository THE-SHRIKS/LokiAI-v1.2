import { describe, expect, it } from 'vitest';
import * as ids from '../src/ids.js';
import { ID_MAX_LENGTH, DIGEST_MAX_LENGTH } from '../src/brand.js';
import { expectInvalid, expectValid } from './helpers.js';

/**
 * Every branded ID schema `ids.ts` exports, keyed by export name, alongside
 * whether it is an ID (bounded by `ID_MAX_LENGTH`) or a digest (bounded by
 * `DIGEST_MAX_LENGTH`, minimum 16 chars). Data-driven so every exported
 * schema gets the full valid/missing/wrong-type/oversized matrix from
 * `AGENTS.md`'s acceptance criteria without 24 near-identical copies.
 */
const ID_SCHEMAS = [
  ['WorkspaceIdSchema', ids.WorkspaceIdSchema],
  ['RuntimeIdSchema', ids.RuntimeIdSchema],
  ['DeviceIdSchema', ids.DeviceIdSchema],
  ['ConnectorIdSchema', ids.ConnectorIdSchema],
  ['AgentConnectionIdSchema', ids.AgentConnectionIdSchema],
  ['AgentRunIdSchema', ids.AgentRunIdSchema],
  ['MonitoringRuleIdSchema', ids.MonitoringRuleIdSchema],
  ['IncidentIdSchema', ids.IncidentIdSchema],
  ['ArtifactIdSchema', ids.ArtifactIdSchema],
  ['DeploymentIdSchema', ids.DeploymentIdSchema],
  ['ApprovalIdSchema', ids.ApprovalIdSchema],
  ['ActivityIdSchema', ids.ActivityIdSchema],
  ['EnrollmentIdSchema', ids.EnrollmentIdSchema],
  ['EnrollmentChallengeIdSchema', ids.EnrollmentChallengeIdSchema],
  ['BuildIdSchema', ids.BuildIdSchema],
  ['JobIdSchema', ids.JobIdSchema],
  ['NotificationIdSchema', ids.NotificationIdSchema],
  ['WorkIntentIdSchema', ids.WorkIntentIdSchema],
  ['MessageIdSchema', ids.MessageIdSchema],
  ['CorrelationIdSchema', ids.CorrelationIdSchema],
  ['CausationIdSchema', ids.CausationIdSchema],
] as const;

const DIGEST_SCHEMAS = [
  ['ArtifactDigestSchema', ids.ArtifactDigestSchema],
  ['BoundRequestDigestSchema', ids.BoundRequestDigestSchema],
  ['ToolchainDigestSchema', ids.ToolchainDigestSchema],
  ['OutputDigestSchema', ids.OutputDigestSchema],
] as const;

describe.each(ID_SCHEMAS)('%s', (_name, schema) => {
  it('accepts a well-formed opaque identifier', () => {
    expectValid(schema, 'a1b2c3-example_id.42:v1');
  });

  it('rejects an empty string', () => {
    expectInvalid(schema, '');
  });

  it('rejects a missing value', () => {
    expectInvalid(schema, undefined);
  });

  it('rejects a wrong type', () => {
    expectInvalid(schema, 123);
    expectInvalid(schema, { id: 'x' });
    expectInvalid(schema, ['x']);
  });

  it('rejects an oversized payload', () => {
    expectInvalid(schema, 'a'.repeat(ID_MAX_LENGTH + 1));
  });

  it('rejects unsafe characters', () => {
    expectInvalid(schema, '../../etc/passwd');
    expectInvalid(schema, 'id;DROP TABLE devices');
  });
});

describe.each(DIGEST_SCHEMAS)('%s', (_name, schema) => {
  const validDigest = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85';

  it('accepts a well-formed digest', () => {
    expectValid(schema, validDigest);
  });

  it('rejects an empty string', () => {
    expectInvalid(schema, '');
  });

  it('rejects a too-short value', () => {
    expectInvalid(schema, 'short');
  });

  it('rejects a wrong type', () => {
    expectInvalid(schema, 999);
    expectInvalid(schema, null);
  });

  it('rejects an oversized payload', () => {
    expectInvalid(schema, 'a'.repeat(DIGEST_MAX_LENGTH + 1));
  });
});

describe('id branding', () => {
  it('produces structurally distinct TypeScript types for different ID kinds', () => {
    const deviceId = ids.DeviceIdSchema.parse('device-1');
    const workspaceId = ids.WorkspaceIdSchema.parse('workspace-1');

    function requiresDeviceId(id: ids.DeviceId): string {
      return id;
    }

    // Compiles only because `deviceId` really is branded `DeviceId`.
    expect(requiresDeviceId(deviceId)).toBe('device-1');
    // The following would be a compile error if uncommented, proving the
    // brand prevents ID-type confusion at compile time (not re-checked at
    // runtime, since branding is a TypeScript-only construct):
    // requiresDeviceId(workspaceId);
    expect(workspaceId).toBe('workspace-1');
  });
});
