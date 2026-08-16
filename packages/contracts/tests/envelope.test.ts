import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { ACTOR_TYPES, ActorSchema, ActorTypeSchema, envelopeSchema } from '../src/envelope.js';
import { expectInvalid, expectValid, omit } from './helpers.js';

describe('ActorTypeSchema', () => {
  it('accepts every documented actor type', () => {
    for (const type of ACTOR_TYPES) {
      expectValid(ActorTypeSchema, type);
    }
  });

  it('rejects an unknown enum value', () => {
    expectInvalid(ActorTypeSchema, 'superuser');
  });
});

describe('ActorSchema', () => {
  it('accepts a minimal actor without a display name', () => {
    expectValid(ActorSchema, { actor_type: 'runtime', actor_id: 'runtime-1' });
  });

  it('accepts an actor with a display name', () => {
    expectValid(ActorSchema, {
      actor_type: 'human_user',
      actor_id: 'user-42',
      display_name: 'Ada Lovelace',
    });
  });

  it('rejects a missing actor_id', () => {
    expectInvalid(ActorSchema, { actor_type: 'runtime' });
  });

  it('rejects an unknown actor_type', () => {
    expectInvalid(ActorSchema, { actor_type: 'bot', actor_id: 'x' });
  });

  it('rejects a wrong type for actor_id', () => {
    expectInvalid(ActorSchema, { actor_type: 'runtime', actor_id: 123 });
  });
});

describe('envelopeSchema', () => {
  const payload = z.object({ message: z.string() });
  const schema = envelopeSchema(payload);

  const valid = {
    protocol_version: '1.0.0',
    message_id: 'msg-1',
    correlation_id: 'corr-1',
    workspace_id: 'workspace-1',
    occurred_at: '2026-08-15T12:00:00Z',
    actor: { actor_type: 'runtime', actor_id: 'runtime-1' },
    payload: { message: 'hello' },
  };

  it('accepts a well-formed envelope without an optional causation_id', () => {
    const parsed = expectValid(schema, valid);
    expect(parsed.payload.message).toBe('hello');
  });

  it('accepts a well-formed envelope with causation_id', () => {
    expectValid(schema, { ...valid, causation_id: 'cause-1' });
  });

  it('rejects a missing required field (workspace_id)', () => {
    expectInvalid(schema, omit(valid, 'workspace_id'));
  });

  it('rejects a non-UTC occurred_at', () => {
    expectInvalid(schema, { ...valid, occurred_at: '2026-08-15T12:00:00+02:00' });
  });

  it('rejects a payload that fails the caller-supplied payload schema', () => {
    expectInvalid(schema, { ...valid, payload: { message: 42 } });
  });

  it('rejects an invalid nested actor', () => {
    expectInvalid(schema, { ...valid, actor: { actor_type: 'unknown_type', actor_id: 'x' } });
  });

  it('rejects an oversized protocol_version', () => {
    expectInvalid(schema, { ...valid, protocol_version: 'v'.repeat(51) });
  });
});
