import { PassThrough } from 'node:stream';
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_MAX_FRAME_BYTES,
  FrameMalformedError,
  FrameTooLargeError,
  FrameTruncatedError,
  encodeFrame,
  readFrames,
} from '../src/frame.js';

const NIL_UUID = '00000000-0000-0000-0000-000000000000';

function samplePing(): import('@lokiai/contracts').WireMessage {
  return {
    type: 'Call',
    correlation_id: NIL_UUID as never,
    deadline_unix_millis: 123,
    method: { method: 'Ping', params: { payload: 'hello' } },
  };
}

async function collect<T>(iterable: AsyncGenerator<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const item of iterable) {
    out.push(item);
  }
  return out;
}

describe('ipc-client frame codec (mirrors crates/loki-ipc/src/frame.rs)', () => {
  it('write then read round-trips one frame', async () => {
    const stream = new PassThrough();
    stream.end(encodeFrame(samplePing()));
    const [decoded] = await collect(readFrames(stream));
    expect(decoded.type).toBe('Call');
  });

  it('multiple frames on one stream are read in order', async () => {
    const stream = new PassThrough();
    const messages = Array.from({ length: 5 }, (_, i) => ({
      type: 'CancelCall' as const,
      correlation_id: NIL_UUID as never,
      seq: i,
    }));
    for (const message of messages) {
      stream.write(encodeFrame({ type: 'CancelCall', correlation_id: message.correlation_id }));
    }
    stream.end();
    const decoded = await collect(readFrames(stream));
    expect(decoded).toHaveLength(5);
    for (const message of decoded) {
      expect(message.type).toBe('CancelCall');
    }
  });

  it('clean close between frames ends the generator normally, not as an error', async () => {
    const stream = new PassThrough();
    stream.end();
    const decoded = await collect(readFrames(stream));
    expect(decoded).toHaveLength(0);
  });

  it('a truncated length prefix throws FrameTruncatedError, not a silent empty result', async () => {
    const stream = new PassThrough();
    stream.end(Buffer.from([1, 2]));
    await expect(collect(readFrames(stream))).rejects.toBeInstanceOf(FrameTruncatedError);
  });

  it('a truncated payload throws FrameTruncatedError', async () => {
    const stream = new PassThrough();
    const header = Buffer.alloc(4);
    header.writeUInt32LE(100, 0);
    stream.end(Buffer.concat([header, Buffer.from('short')]));
    await expect(collect(readFrames(stream))).rejects.toBeInstanceOf(FrameTruncatedError);
  });

  it('an oversized declared length is rejected promptly without allocating/reading the claimed payload', async () => {
    const stream = new PassThrough();
    const header = Buffer.alloc(4);
    header.writeUInt32LE(1024 * 1024 * 1024, 0); // claims 1 GiB, never sent
    stream.write(header);

    const start = Date.now();
    await expect(collect(readFrames(stream))).rejects.toBeInstanceOf(FrameTooLargeError);
    expect(Date.now() - start).toBeLessThan(2_000);
    stream.end();
  });

  it('invalid JSON payload throws FrameMalformedError', async () => {
    const stream = new PassThrough();
    const payload = Buffer.from('not json', 'utf8');
    const header = Buffer.alloc(4);
    header.writeUInt32LE(payload.length, 0);
    stream.end(Buffer.concat([header, payload]));
    await expect(collect(readFrames(stream))).rejects.toBeInstanceOf(FrameMalformedError);
  });

  it('valid JSON that is not a WireMessage throws FrameMalformedError', async () => {
    const stream = new PassThrough();
    const payload = Buffer.from(JSON.stringify({ totally: 'unrelated' }), 'utf8');
    const header = Buffer.alloc(4);
    header.writeUInt32LE(payload.length, 0);
    stream.end(Buffer.concat([header, payload]));
    await expect(collect(readFrames(stream))).rejects.toBeInstanceOf(FrameMalformedError);
  });

  it('encodeFrame rejects a payload larger than the default maximum before returning any bytes', () => {
    const huge = {
      type: 'Call' as const,
      correlation_id: NIL_UUID as never,
      deadline_unix_millis: 0,
      method: {
        method: 'Ping' as const,
        params: { payload: 'x'.repeat(DEFAULT_MAX_FRAME_BYTES + 1) },
      },
    };
    expect(() => encodeFrame(huge)).toThrow(FrameTooLargeError);
  });
});
