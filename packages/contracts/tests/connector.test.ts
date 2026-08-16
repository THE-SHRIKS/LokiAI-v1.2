import { describe, expect, it } from 'vitest';
import {
  CAPABILITY_RISK_LEVELS,
  CapabilityRiskLevelSchema,
  ConnectorCapabilityDescriptorSchema,
  ConnectorCapabilityIdSchema,
  HEALTH_CHECK_STATUSES,
  HealthCheckStatusSchema,
  KNOWN_CONNECTOR_CAPABILITY_IDS,
  KnownConnectorCapabilityIdSchema,
  SUPPORT_TIERS,
  SupportTierSchema,
} from '../src/connector.js';
import { expectInvalid, expectValid, omit } from './helpers.js';

describe('SupportTierSchema', () => {
  it('accepts every documented support tier', () => {
    for (const tier of SUPPORT_TIERS) {
      expectValid(SupportTierSchema, tier);
    }
  });

  it('rejects an unknown enum value', () => {
    expectInvalid(SupportTierSchema, 'beta');
  });

  it('rejects Title-Case (matches the lowercase wire convention, not the table heading style)', () => {
    expectInvalid(SupportTierSchema, 'Certified');
  });

  it('rejects a wrong type', () => {
    expectInvalid(SupportTierSchema, 1);
  });
});

describe('CapabilityRiskLevelSchema', () => {
  it('accepts every documented risk level', () => {
    for (const level of CAPABILITY_RISK_LEVELS) {
      expectValid(CapabilityRiskLevelSchema, level);
    }
  });

  it('rejects an unknown enum value', () => {
    expectInvalid(CapabilityRiskLevelSchema, 'critical');
  });

  it('rejects a missing value', () => {
    expectInvalid(CapabilityRiskLevelSchema, undefined);
  });
});

describe('HealthCheckStatusSchema', () => {
  it('accepts every documented status', () => {
    for (const status of HEALTH_CHECK_STATUSES) {
      expectValid(HealthCheckStatusSchema, status);
    }
  });

  it('rejects an unknown enum value', () => {
    expectInvalid(HealthCheckStatusSchema, 'ok');
  });
});

describe('ConnectorCapabilityIdSchema', () => {
  it('accepts every known capability id slug', () => {
    for (const id of KNOWN_CONNECTOR_CAPABILITY_IDS) {
      expectValid(ConnectorCapabilityIdSchema, id);
    }
  });

  it('accepts an unlisted but well-formed slug (non-exhaustive vocabulary)', () => {
    expectValid(ConnectorCapabilityIdSchema, 'future_capability');
  });

  it('rejects a non-slug string', () => {
    expectInvalid(ConnectorCapabilityIdSchema, 'Not A Slug');
    expectInvalid(ConnectorCapabilityIdSchema, 'UPPERCASE');
    expectInvalid(ConnectorCapabilityIdSchema, '1_starts_with_digit');
  });

  it('rejects an empty string', () => {
    expectInvalid(ConnectorCapabilityIdSchema, '');
  });
});

describe('KnownConnectorCapabilityIdSchema', () => {
  it('accepts every known capability id slug', () => {
    for (const id of KNOWN_CONNECTOR_CAPABILITY_IDS) {
      expectValid(KnownConnectorCapabilityIdSchema, id);
    }
  });

  it('rejects an unlisted slug (closed enum, unlike ConnectorCapabilityIdSchema)', () => {
    expectInvalid(KnownConnectorCapabilityIdSchema, 'future_capability');
  });

  it('rejects a wrong type', () => {
    expectInvalid(KnownConnectorCapabilityIdSchema, 42);
  });
});

describe('ConnectorCapabilityDescriptorSchema', () => {
  const valid = {
    capability_id: 'flash_artifact',
    schema_ref: 'connector.flash_artifact.v1',
    schema_version: '1.0.0',
    risk: 'high',
    required_state: ['ready'],
    exclusive_access: true,
    expected_interruption: true,
    timeout_ms: 120_000,
    verification_required: true,
    recovery_supported: true,
  };

  it('accepts a well-formed capability descriptor', () => {
    const parsed = expectValid(ConnectorCapabilityDescriptorSchema, valid);
    expect(parsed.capability_id).toBe('flash_artifact');
  });

  it('rejects a missing required field', () => {
    expectInvalid(ConnectorCapabilityDescriptorSchema, omit(valid, 'risk'));
  });

  it('rejects a wrong type for a boolean field', () => {
    expectInvalid(ConnectorCapabilityDescriptorSchema, { ...valid, exclusive_access: 'yes' });
  });

  it('rejects an unknown enum value for required_state', () => {
    expectInvalid(ConnectorCapabilityDescriptorSchema, {
      ...valid,
      required_state: ['powered-on'],
    });
  });

  it('rejects an empty required_state array', () => {
    expectInvalid(ConnectorCapabilityDescriptorSchema, { ...valid, required_state: [] });
  });

  it('rejects a non-positive timeout', () => {
    expectInvalid(ConnectorCapabilityDescriptorSchema, { ...valid, timeout_ms: 0 });
    expectInvalid(ConnectorCapabilityDescriptorSchema, { ...valid, timeout_ms: -1 });
  });

  it('rejects an oversized payload for a bounded string field', () => {
    expectInvalid(ConnectorCapabilityDescriptorSchema, { ...valid, schema_ref: 'x'.repeat(300) });
  });
});
