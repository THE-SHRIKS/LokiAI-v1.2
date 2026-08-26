import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
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
  IdentityProbeResultSchema,
  PublicDeviceIdentitySchema,
  TransportCandidateSchema,
  redactIdentityProbeResult,
  redactSensitiveLocator,
  redactTransportCandidate,
  type IdentityProbeResult,
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

describe('device-derived connector identity contract', () => {
  const fixture = JSON.parse(
    readFileSync(
      fileURLToPath(new URL('../fixtures/connector-identity-v1.json', import.meta.url)),
      'utf8',
    ),
  ) as Record<string, unknown>;

  it('accepts the shared verified cross-language fixture exactly', () => {
    expect(IdentityProbeResultSchema.parse(fixture)).toEqual(fixture);
  });

  it('never accepts a provisional candidate as stable identity', () => {
    const candidate = fixture.candidate as Record<string, unknown>;
    expectInvalid(PublicDeviceIdentitySchema, candidate);
    expectInvalid(TransportCandidateSchema, { ...candidate, candidate_id_stability: 'stable' });
  });

  it.each(['unknown', 'partial', 'ambiguous', 'conflicting'])(
    'fails closed when %s carries a public identity',
    (status) => expectInvalid(IdentityProbeResultSchema, { ...fixture, status }),
  );

  it('requires multiple evidence items for ambiguous and conflicting states', () => {
    for (const status of ['ambiguous', 'conflicting']) {
      expectInvalid(IdentityProbeResultSchema, { ...fixture, status, public_identity: null });
    }
  });

  it('rejects conflicting fingerprints in a verified result', () => {
    const evidence = fixture.evidence as Array<Record<string, unknown>>;
    expectInvalid(IdentityProbeResultSchema, {
      ...fixture,
      evidence: [...evidence, { ...evidence[0], fingerprint_digest: 'b'.repeat(64) }],
    });
  });

  it('rejects a forged evidence digest independently of resolution status', () => {
    const evidence = fixture.evidence as Array<Record<string, unknown>>;
    expectInvalid(IdentityProbeResultSchema, {
      ...fixture,
      evidence: [{ ...evidence[0], evidence_digest: 'b'.repeat(64) }],
    });
  });

  it('rejects absent raw-material handling, extra fields, and bad UTC time', () => {
    const candidate = fixture.candidate as Record<string, unknown>;
    const locator = candidate.locator as Record<string, unknown>;
    const evidence = fixture.evidence as Array<Record<string, unknown>>;
    expectInvalid(IdentityProbeResultSchema, {
      ...fixture,
      candidate: { ...candidate, locator: omit(locator, 'sensitivity') },
    });
    expectInvalid(IdentityProbeResultSchema, { ...fixture, unexpected: true });
    expectInvalid(IdentityProbeResultSchema, {
      ...fixture,
      candidate: { ...candidate, discovered_at: 'not-utc' },
    });
    expectInvalid(IdentityProbeResultSchema, {
      ...fixture,
      evidence: [{ ...evidence[0], raw_material_handling: undefined }],
    });
  });

  describe('sensitive locator redaction', () => {
    // Wire parsing (`IdentityProbeResultSchema`) legitimately carries the
    // real locator once, for the one case a connector needs it to open the
    // device — confirmed by the shared fixture round-trip test above. Any
    // other use (logging, evidence persistence) must go through the
    // explicit redaction helpers instead of the raw parsed object.
    const parsed = IdentityProbeResultSchema.parse(fixture) as IdentityProbeResult;

    it('parsed wire data still carries the real locator (the one legitimate transport use)', () => {
      expect(parsed.candidate.locator.value).toBe('fixture-locator-not-a-real-device');
    });

    it('redactSensitiveLocator strips the raw value but preserves security semantics', () => {
      const redacted = redactSensitiveLocator(parsed.candidate.locator);
      expect(redacted.value).toBe('[REDACTED]');
      expect(redacted.sensitivity).toBe('sensitive');
      expect(redacted.stability).toBe('unstable');
    });

    it('redactTransportCandidate strips the raw locator but preserves every other field', () => {
      const redacted = redactTransportCandidate(parsed.candidate);
      expect(redacted.locator.value).toBe('[REDACTED]');
      expect({ ...redacted, locator: parsed.candidate.locator }).toEqual(parsed.candidate);
    });

    it('redactIdentityProbeResult strips the raw locator from the whole result and never leaks it via JSON.stringify', () => {
      const redacted = redactIdentityProbeResult(parsed);
      expect(redacted.candidate.locator.value).toBe('[REDACTED]');
      const serialized = JSON.stringify(redacted);
      expect(serialized).not.toContain('fixture-locator-not-a-real-device');
      expect(serialized).toContain('[REDACTED]');
      // Nothing else about the result is altered.
      expect({ ...redacted, candidate: parsed.candidate }).toEqual(parsed);
    });

    it('the unredacted parsed object DOES leak the raw locator via plain JSON.stringify, which is exactly why callers must opt into redaction', () => {
      expect(JSON.stringify(parsed)).toContain('fixture-locator-not-a-real-device');
    });
  });
});
