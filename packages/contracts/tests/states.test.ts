import { describe, expect, it } from 'vitest';
import type { z } from 'zod';
import {
  DEVICE_LIFECYCLE_STATES,
  DeviceLifecycleStateSchema,
  DEPLOYMENT_STATES,
  DeploymentStateSchema,
  INCIDENT_STATES,
  INCIDENT_TRANSITIONS,
  IncidentStateSchema,
  JOB_STATES,
  JobStateSchema,
  ONBOARDING_STEPS,
  ONBOARDING_TRANSITIONS,
  OnboardingStepSchema,
  RUNTIME_CONNECTION_STATES,
  RuntimeConnectionStateSchema,
} from '../src/states.js';
import { expectInvalid, expectValid } from './helpers.js';

type EnumCase = readonly [string, readonly string[], z.ZodTypeAny];

const ENUM_CASES: readonly EnumCase[] = [
  ['RuntimeConnectionStateSchema', RUNTIME_CONNECTION_STATES, RuntimeConnectionStateSchema],
  ['JobStateSchema', JOB_STATES, JobStateSchema],
  ['DeploymentStateSchema', DEPLOYMENT_STATES, DeploymentStateSchema],
  ['IncidentStateSchema', INCIDENT_STATES, IncidentStateSchema],
  ['OnboardingStepSchema', ONBOARDING_STEPS, OnboardingStepSchema],
  ['DeviceLifecycleStateSchema', DEVICE_LIFECYCLE_STATES, DeviceLifecycleStateSchema],
];

describe.each(ENUM_CASES)('%s', (_name, values, schema) => {
  it('accepts every documented state value', () => {
    for (const value of values) {
      expectValid(schema, value);
    }
  });

  it('rejects an unknown enum value', () => {
    expectInvalid(schema, 'not-a-real-state');
    expectInvalid(schema, `${values[0]}-typo`);
  });

  it('rejects a wrong type', () => {
    expectInvalid(schema, 42);
    expectInvalid(schema, null);
    expectInvalid(schema, { state: values[0] });
  });

  it('rejects a missing value', () => {
    expectInvalid(schema, undefined);
  });

  it('is case-sensitive (rejects a differently-cased valid token)', () => {
    const [firstValue] = values;
    expectInvalid(schema, firstValue.toUpperCase());
  });
});

describe('RuntimeConnectionState', () => {
  it('preserves the exact hyphenated multi-word token from Docs/SPEC.md 4.3', () => {
    expect(RUNTIME_CONNECTION_STATES).toContain('version-incompatible');
  });
});

describe('IncidentState transitions', () => {
  it('has an entry for every incident state', () => {
    for (const state of INCIDENT_STATES) {
      expect(INCIDENT_TRANSITIONS).toHaveProperty(state);
    }
  });

  it('only points to other valid incident states', () => {
    for (const targets of Object.values(INCIDENT_TRANSITIONS)) {
      for (const target of targets) {
        expect(INCIDENT_STATES).toContain(target);
      }
    }
  });

  it('matches every edge transcribed from the Section 4.6 Mermaid diagram', () => {
    expect(INCIDENT_TRANSITIONS.open).toEqual(['investigating']);
    expect(INCIDENT_TRANSITIONS.investigating).toEqual(['waiting-approval', 'verifying']);
    expect(INCIDENT_TRANSITIONS['waiting-approval']).toEqual(['investigating', 'executing']);
    expect(INCIDENT_TRANSITIONS.executing).toEqual(['verifying', 'investigating']);
    expect(INCIDENT_TRANSITIONS.verifying).toEqual(['resolved', 'investigating']);
    expect(INCIDENT_TRANSITIONS.resolved).toEqual(['open', 'closed']);
    expect(INCIDENT_TRANSITIONS.closed).toEqual([]);
  });
});

describe('OnboardingStep transitions', () => {
  it('has an entry for every onboarding step', () => {
    for (const step of ONBOARDING_STEPS) {
      expect(ONBOARDING_TRANSITIONS).toHaveProperty(step);
    }
  });

  it('only points to other valid onboarding steps', () => {
    for (const targets of Object.values(ONBOARDING_TRANSITIONS)) {
      for (const target of targets) {
        expect(ONBOARDING_STEPS).toContain(target);
      }
    }
  });

  it('branches device-choice into the existing-device and new-project paths', () => {
    expect(ONBOARDING_TRANSITIONS['device-choice']).toEqual(['existing-device', 'new-project']);
  });

  it('converges both onboarding paths on review, then ready', () => {
    expect(ONBOARDING_TRANSITIONS.health).toEqual(['review']);
    expect(ONBOARDING_TRANSITIONS['project-target']).toEqual(['review']);
    expect(ONBOARDING_TRANSITIONS.review).toEqual(['ready']);
    expect(ONBOARDING_TRANSITIONS.ready).toEqual([]);
  });
});
