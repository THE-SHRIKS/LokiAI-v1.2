import { describe, expect, it } from 'vitest';
import {
  CORE_CAPABILITY,
  CallOutcomeSchema,
  HandshakeRejectReasonSchema,
  MethodResultSchema,
  MethodSchema,
  PROTOCOL_VERSION,
  ProtocolVersionSchema,
  RpcErrorSchema,
  StreamMethodSchema,
  StreamOutcomeSchema,
  WireMessageSchema,
} from '../src/ipcProtocol.js';

const NIL_UUID = '00000000-0000-0000-0000-000000000000';

describe('ipcProtocol — envelope-level wire contract (P1-NODE-IPC-CLIENT-001)', () => {
  it('PROTOCOL_VERSION matches crates/loki-ipc/src/protocol.rs::PROTOCOL_VERSION (1.1)', () => {
    expect(PROTOCOL_VERSION).toEqual({ major: 1, minor: 1 });
    expect(ProtocolVersionSchema.parse(PROTOCOL_VERSION)).toEqual(PROTOCOL_VERSION);
  });

  it('CORE_CAPABILITY matches the Rust constant exactly', () => {
    expect(CORE_CAPABILITY).toBe('loki-ipc.core.v1');
  });

  it('Method: the Ping variant keeps its capitalized tag (rename override), not snake_case', () => {
    const parsed = MethodSchema.parse({ method: 'Ping', params: { payload: 'hello' } });
    expect(parsed).toEqual({ method: 'Ping', params: { payload: 'hello' } });
  });

  it('Method: a business variant is exactly the businessRpc.ts request shape (snake_case tag)', () => {
    const parsed = MethodSchema.parse({ method: 'runtime_get_status', params: {} });
    expect(parsed).toMatchObject({ method: 'runtime_get_status' });
  });

  it('Method rejects a lowercase "ping" — Rust only accepts the renamed "Ping"', () => {
    expect(() => MethodSchema.parse({ method: 'ping', params: { payload: 'x' } })).toThrow();
  });

  it('StreamMethod: EchoStream keeps its capitalized tag; BusinessEvents is snake_case "business_events"', () => {
    expect(
      StreamMethodSchema.parse({ method: 'EchoStream', params: { payload: 'x', count: 3 } }),
    ).toMatchObject({ method: 'EchoStream' });
    expect(
      StreamMethodSchema.parse({
        method: 'business_events',
        params: { stream: 'jobs', entity_id: 'job-1', max_items: 10 },
      }),
    ).toMatchObject({ method: 'business_events' });
  });

  it('MethodResult: no rename_all — tags are the bare Rust identifiers "Ping"/"Business"', () => {
    const ping = MethodResultSchema.parse({
      method: 'Ping',
      result: { payload: 'hello', server_unix_millis: 42 },
    });
    expect(ping).toMatchObject({ method: 'Ping' });
    expect(() =>
      MethodResultSchema.parse({ method: 'ping', result: { payload: 'x', server_unix_millis: 1 } }),
    ).toThrow();
  });

  it('CallOutcome: outcome tag is snake_case ok/err', () => {
    const ok = CallOutcomeSchema.parse({
      outcome: 'ok',
      result: { method: 'Ping', result: { payload: 'x', server_unix_millis: 1 } },
    });
    expect(ok.outcome).toBe('ok');
    const err = CallOutcomeSchema.parse({ outcome: 'err', error: { kind: 'deadline_exceeded' } });
    expect(err.outcome).toBe('err');
  });

  it('StreamOutcome round-trips both ok/err', () => {
    const ok = StreamOutcomeSchema.parse({
      outcome: 'ok',
      summary: { method: 'EchoStream', summary: { items_sent: 3 } },
    });
    expect(ok.outcome).toBe('ok');
    const err = StreamOutcomeSchema.parse({ outcome: 'err', error: { kind: 'cancelled' } });
    expect(err.outcome).toBe('err');
  });

  it('every RpcError variant parses with its exact snake_case kind tag', () => {
    const samples: unknown[] = [
      { kind: 'deadline_exceeded' },
      { kind: 'cancelled' },
      { kind: 'invalid_request', message: 'bad' },
      { kind: 'missing_capability', required: 'loki-ipc.business.v1' },
      { kind: 'permission_denied' },
      { kind: 'credential_revalidation_failed' },
      { kind: 'unsupported_method', method: 'restart_request' },
      { kind: 'internal', message: 'panic' },
    ];
    for (const sample of samples) {
      expect(RpcErrorSchema.parse(sample)).toEqual(sample);
    }
  });

  it('every HandshakeRejectReason variant parses with its exact snake_case reason tag', () => {
    expect(
      HandshakeRejectReasonSchema.parse({
        reason: 'version_mismatch',
        server_version: { major: 2, minor: 0 },
        client_version: { major: 1, minor: 0 },
      }).reason,
    ).toBe('version_mismatch');
    expect(
      HandshakeRejectReasonSchema.parse({ reason: 'missing_capability', required: 'x' }).reason,
    ).toBe('missing_capability');
    expect(HandshakeRejectReasonSchema.parse({ reason: 'authentication_failed' }).reason).toBe(
      'authentication_failed',
    );
    expect(HandshakeRejectReasonSchema.parse({ reason: 'stale_nonce' }).reason).toBe('stale_nonce');
    expect(
      HandshakeRejectReasonSchema.parse({ reason: 'malformed_hello', message: 'oops' }).reason,
    ).toBe('malformed_hello');
  });

  it('WireMessage: every variant tag is the bare Rust identifier under "type" (no rename_all)', () => {
    const challenge = WireMessageSchema.parse({
      type: 'Challenge',
      server_protocol_version: { major: 1, minor: 1 },
      server_capabilities: [CORE_CAPABILITY],
      server_run_id: NIL_UUID,
      nonce: NIL_UUID,
    });
    expect(challenge.type).toBe('Challenge');

    const hello = WireMessageSchema.parse({
      type: 'Hello',
      client_protocol_version: { major: 1, minor: 1 },
      client_capabilities: [CORE_CAPABILITY],
      client_type: 'test-client',
      client_version: '0.0.0',
      nonce: NIL_UUID,
      token: 'a-token-value',
    });
    expect(hello.type).toBe('Hello');

    const helloAck = WireMessageSchema.parse({
      type: 'HelloAck',
      negotiated_capabilities: [CORE_CAPABILITY],
      session_id: NIL_UUID,
      principal: null,
    });
    expect(helloAck.type).toBe('HelloAck');

    const helloAckWithPrincipal = WireMessageSchema.parse({
      type: 'HelloAck',
      negotiated_capabilities: [CORE_CAPABILITY],
      session_id: NIL_UUID,
      principal: {
        kind: 'desktop_installation',
        principal_id: 'desktop-1',
        authorized_entity_ids: ['dev:1'],
        authority: ['read_runtime'],
      },
    });
    expect(helloAckWithPrincipal.type).toBe('HelloAck');

    const helloReject = WireMessageSchema.parse({
      type: 'HelloReject',
      reason: { reason: 'authentication_failed' },
      server_protocol_version: { major: 1, minor: 1 },
    });
    expect(helloReject.type).toBe('HelloReject');

    const call = WireMessageSchema.parse({
      type: 'Call',
      correlation_id: NIL_UUID,
      deadline_unix_millis: 123,
      method: { method: 'Ping', params: { payload: 'hi' } },
    });
    expect(call.type).toBe('Call');

    const callResult = WireMessageSchema.parse({
      type: 'CallResult',
      correlation_id: NIL_UUID,
      outcome: {
        outcome: 'ok',
        result: { method: 'Ping', result: { payload: 'hi', server_unix_millis: 1 } },
      },
    });
    expect(callResult.type).toBe('CallResult');

    const callStream = WireMessageSchema.parse({
      type: 'CallStream',
      correlation_id: NIL_UUID,
      deadline_unix_millis: 123,
      method: { method: 'EchoStream', params: { payload: 'hi', count: 1 } },
    });
    expect(callStream.type).toBe('CallStream');

    const streamItem = WireMessageSchema.parse({
      type: 'StreamItem',
      correlation_id: NIL_UUID,
      seq: 0,
      item: { method: 'EchoStream', item: { seq: 0, payload: 'hi' } },
    });
    expect(streamItem.type).toBe('StreamItem');

    const streamEnd = WireMessageSchema.parse({
      type: 'StreamEnd',
      correlation_id: NIL_UUID,
      outcome: { outcome: 'ok', summary: { method: 'EchoStream', summary: { items_sent: 1 } } },
    });
    expect(streamEnd.type).toBe('StreamEnd');

    const cancelCall = WireMessageSchema.parse({ type: 'CancelCall', correlation_id: NIL_UUID });
    expect(cancelCall.type).toBe('CancelCall');
  });

  it('WireMessage rejects an unknown "type" tag as a clear parse error, not a silent pass-through', () => {
    expect(() =>
      WireMessageSchema.parse({ type: 'NotARealMessage', correlation_id: NIL_UUID }),
    ).toThrow();
  });
});
