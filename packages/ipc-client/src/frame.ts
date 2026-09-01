/**
 * `P1-NODE-IPC-CLIENT-001`: the exact `[u32 LE length][JSON payload]` frame
 * codec `crates/loki-ipc/src/frame.rs` uses, reimplemented here (Node has no
 * shared-crate seam into that Rust module) — a genuine second
 * implementation of the same wire codec, proven wire-compatible by this
 * package's own integration tests against a real running `loki-ipc` server,
 * not assumed compatible by construction.
 */

import type { Readable } from 'node:stream';
import { WireMessageSchema, type WireMessage } from '@lokiai/contracts';

/** Mirrors `loki_ipc::frame::DEFAULT_MAX_FRAME_BYTES` (4 MiB). */
export const DEFAULT_MAX_FRAME_BYTES = 4 * 1024 * 1024;

export class FrameTooLargeError extends Error {
  constructor(
    public readonly len: number,
    public readonly max: number,
  ) {
    super(`frame too large: ${len} bytes (maximum ${max})`);
    this.name = 'FrameTooLargeError';
  }
}

/** A connection ended with a partial length prefix or partial payload
 * in-flight — distinct from a clean between-frames close, matching
 * `frame.rs::read_frame`'s own "truncated frame" vs. `Ok(None)` distinction. */
export class FrameTruncatedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'FrameTruncatedError';
  }
}

/** The payload was not valid UTF-8 JSON, or did not match the
 * `WireMessage` envelope schema. */
export class FrameMalformedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'FrameMalformedError';
  }
}

/**
 * Serializes and encodes one frame. Throws {@link FrameTooLargeError}
 * without ever writing partial bytes if the serialized payload exceeds
 * `maxFrameBytes` — mirrors `frame.rs::write_frame`'s own bound check
 * before any I/O.
 */
export function encodeFrame(message: WireMessage, maxFrameBytes = DEFAULT_MAX_FRAME_BYTES): Buffer {
  const payload = Buffer.from(JSON.stringify(message), 'utf8');
  if (payload.length > maxFrameBytes) {
    throw new FrameTooLargeError(payload.length, maxFrameBytes);
  }
  const header = Buffer.alloc(4);
  header.writeUInt32LE(payload.length, 0);
  return Buffer.concat([header, payload]);
}

/**
 * Reads and decodes every frame on `stream` as they arrive, in order.
 * Yields zero or more {@link WireMessage}s, then returns normally exactly
 * when the peer closed the connection cleanly *between* frames — mirroring
 * `frame.rs::read_frame`'s `Ok(None)` case. A peer that closes mid-length-
 * prefix or mid-payload, an oversized declared length (rejected **before**
 * the claimed payload is read/allocated — mirrors `frame.rs`'s own
 * "oversized length prefix... without allocating" behavior), or malformed
 * JSON/schema-mismatched payload each throw a specific, typed error instead
 * of silently ending the stream.
 */
export async function* readFrames(
  stream: Readable,
  maxFrameBytes = DEFAULT_MAX_FRAME_BYTES,
): AsyncGenerator<WireMessage, void, void> {
  let buffer = Buffer.alloc(0);

  for await (const chunk of stream as AsyncIterable<Buffer>) {
    buffer = buffer.length > 0 ? Buffer.concat([buffer, chunk]) : Buffer.from(chunk);

    for (;;) {
      if (buffer.length < 4) {
        break;
      }
      const len = buffer.readUInt32LE(0);
      if (len > maxFrameBytes) {
        // Rejected without ever allocating/reading the claimed payload —
        // the declared length alone is enough to reject.
        throw new FrameTooLargeError(len, maxFrameBytes);
      }
      if (buffer.length < 4 + len) {
        break; // wait for more data
      }
      const payloadBytes = buffer.subarray(4, 4 + len);
      buffer = Buffer.from(buffer.subarray(4 + len));

      let parsedJson: unknown;
      try {
        parsedJson = JSON.parse(payloadBytes.toString('utf8'));
      } catch (error) {
        throw new FrameMalformedError(
          `malformed frame payload: not valid JSON (${(error as Error).message})`,
        );
      }
      const result = WireMessageSchema.safeParse(parsedJson);
      if (!result.success) {
        throw new FrameMalformedError(
          `malformed frame payload: does not match the WireMessage envelope schema (${result.error.message})`,
        );
      }
      yield result.data;
    }
  }

  // The underlying stream ended. A non-empty leftover buffer means the
  // connection closed mid-length-prefix or mid-payload — a genuine
  // truncated frame, not an ordinary clean disconnect.
  if (buffer.length > 0) {
    throw new FrameTruncatedError(
      `connection closed mid-frame (${buffer.length} leftover byte(s) never completed a frame)`,
    );
  }
}
