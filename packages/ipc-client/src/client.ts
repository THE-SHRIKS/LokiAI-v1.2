/**
 * `P1-NODE-IPC-CLIENT-001`: the Node named-pipe RPC client itself —
 * connect/handshake, `ping`/`businessCall`/`businessStream`/`cancelCall`,
 * per-call deadline + grace, and reconnect-only-during-initial-connect —
 * matching `crates/loki-ipc/src/client.rs`'s conventions exactly (see that
 * module and this package's own tests for the wire-/behavior-compatibility
 * proof).
 *
 * **No auto-reconnect.** Once a connection is established, this client
 * never silently reconnects after it is lost — a caller must call
 * `connect()` again, exactly matching `client.rs::Client`'s own documented
 * behavior. Grep-verifiable: no retry/backoff loop wraps the post-handshake
 * read/write path below.
 */

import net, { type Socket } from 'node:net';
import crypto from 'node:crypto';
import type {
  BusinessRpcRequest,
  BusinessRpcResult,
  CallOutcome,
  CorrelationId,
  Method,
  ProtocolVersion,
  StreamOutcome,
  WireMessage,
} from '@lokiai/contracts';
import { BUSINESS_CAPABILITY, CORE_CAPABILITY, PROTOCOL_VERSION } from '@lokiai/contracts';
import { loadCredential, type CredentialKind } from '@lokiai/native-credential-store';
import { DEFAULT_MAX_FRAME_BYTES, encodeFrame, readFrames } from './frame.js';
import {
  ClientTimedOutError,
  ConnectFrameError,
  ConnectIoError,
  ConnectTimedOutError,
  ConnectionClosedDuringHandshakeError,
  ConnectionClosedError,
  CredentialLoadError,
  HandshakeRejectedError,
  ProtocolMismatchError,
  RpcCallError,
  UnexpectedHandshakeMessageError,
} from './errors.js';

/** Mirrors `loki_ipc::client::CLIENT_DEADLINE_GRACE` (2000ms) exactly. */
export const CLIENT_DEADLINE_GRACE_MS = 2_000;

/**
 * `WireMessage`'s `correlation_id` fields carry `@lokiai/contracts`'
 * branded `CorrelationId` type (a compile-time-only nominal marker, no
 * runtime representation difference from a plain `string`). This client
 * mints fresh correlation ids locally (`crypto.randomUUID()`), so this cast
 * — not a schema bypass, since every outbound message is still exactly the
 * shape `WireMessageSchema` accepts — is the correct way to attach that
 * brand to a value this module itself produced and knows is well-formed.
 */
function asCorrelationId(value: string): CorrelationId {
  return value as CorrelationId;
}

/** Windows named-pipe "every server instance is currently connected"
 * condition (`ERROR_PIPE_BUSY`, winerror.h 231). Node/libuv do not expose
 * this raw Win32 error code directly on the thrown `Error`; empirically
 * (this package's own tests — see `tests/busyClassifier.test.ts`), Node
 * surfaces a busy named-pipe connect failure as an `Error` whose `code` is
 * `'EBUSY'` or whose message contains "busy" (case-insensitive). This
 * classifier is deliberately permissive across both observed shapes rather
 * than pinned to one exact string, since the underlying libuv error
 * surface is not part of Node's own stable public contract. Documented
 * honestly in this node's handoff: forcing a genuine, deterministic
 * `ERROR_PIPE_BUSY` from a separate Node process against
 * `crates/loki-ipc`'s server (which allows unlimited pipe instances and
 * re-arms a fresh pending instance immediately after every accept) proved
 * empirically elusive in this environment — the retry code path below is
 * real and matches `client.rs::open_with_retry`'s algorithm exactly, but
 * this specific classifier could not be independently confirmed against a
 * forced, reproducible busy condition.
 */
function isBusyPipeError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }
  const code = (error as NodeJS.ErrnoException).code;
  if (code === 'EBUSY') {
    return true;
  }
  return /busy/i.test(error.message);
}

export interface NodeIpcClientConfig {
  pipeName: string;
  /** Which purpose-bound credential kind to present at `Hello`. */
  credentialKind: CredentialKind;
  /** Path to that credential's DPAPI-protected file. */
  credentialPath: string;
  clientType: string;
  clientVersion?: string;
  protocolVersion?: ProtocolVersion;
  capabilities?: ReadonlySet<string>;
  maxFrameBytes?: number;
  /** How long `connect()` retries a busy-pipe connect error before giving up. */
  connectTimeoutMs?: number;
}

interface ResolvedConfig extends Required<Omit<NodeIpcClientConfig, 'capabilities'>> {
  capabilities: ReadonlySet<string>;
}

function resolveConfig(config: NodeIpcClientConfig): ResolvedConfig {
  return {
    pipeName: config.pipeName,
    credentialKind: config.credentialKind,
    credentialPath: config.credentialPath,
    clientType: config.clientType,
    clientVersion: config.clientVersion ?? '0.1.0',
    protocolVersion: config.protocolVersion ?? PROTOCOL_VERSION,
    capabilities: config.capabilities ?? new Set([CORE_CAPABILITY, BUSINESS_CAPABILITY]),
    maxFrameBytes: config.maxFrameBytes ?? DEFAULT_MAX_FRAME_BYTES,
    connectTimeoutMs: config.connectTimeoutMs ?? 5_000,
  };
}

/** Legal capabilities for a given declared `protocolVersion` — mirrors
 * `crates/loki-ipc/src/protocol.rs::capabilities_for_version` exactly. */
function capabilitiesForVersion(version: ProtocolVersion): Set<string> {
  if (version.major === PROTOCOL_VERSION.major && version.minor >= PROTOCOL_VERSION.minor) {
    return new Set([CORE_CAPABILITY, BUSINESS_CAPABILITY]);
  }
  return new Set([CORE_CAPABILITY]);
}

type PendingUnary = {
  kind: 'unary';
  resolve: (outcome: CallOutcome) => void;
  reject: (error: Error) => void;
};

type PendingStream = {
  kind: 'stream';
  push: (event: StreamEvent) => void;
};

type PendingCall = PendingUnary | PendingStream;

export type StreamEvent =
  { type: 'item'; item: unknown } | { type: 'ended'; outcome: StreamOutcome };

export class BusinessStreamHandle implements AsyncIterable<StreamEvent> {
  private readonly queue: StreamEvent[] = [];
  private readonly waiters: Array<(value: IteratorResult<StreamEvent>) => void> = [];
  private ended = false;

  constructor(public readonly correlationId: string) {}

  /** @internal */
  push(event: StreamEvent): void {
    if (this.ended) {
      return;
    }
    if (event.type === 'ended') {
      this.ended = true;
    }
    const waiter = this.waiters.shift();
    if (waiter) {
      waiter({ value: event, done: false });
    } else {
      this.queue.push(event);
    }
  }

  [Symbol.asyncIterator](): AsyncIterator<StreamEvent> {
    return {
      next: (): Promise<IteratorResult<StreamEvent>> => {
        const queued = this.queue.shift();
        if (queued) {
          return Promise.resolve({ value: queued, done: false });
        }
        if (this.ended) {
          return Promise.resolve({ value: undefined, done: true });
        }
        return new Promise((resolve) => this.waiters.push(resolve));
      },
    };
  }
}

/** Mirrors `crates/loki-ipc/src/business.rs`'s method<->result-kind
 * correspondence for the methods this client actually validates —
 * `Client::business_call`'s own defensive "never hand a caller a
 * mismatched typed result" check. */
function resultMatchesMethod(method: BusinessRpcRequest, result: BusinessRpcResult): boolean {
  switch (method.method) {
    case 'runtime_get_status':
      return result.kind === 'runtime_status';
    case 'devices_list':
      return result.kind === 'devices_list';
    case 'device_get':
      return result.kind === 'device_get';
    case 'device_get_health':
      return result.kind === 'device_health';
    case 'restart_request':
      return result.kind === 'mutation' && result.method === 'restart';
    case 'flash_request':
      return result.kind === 'mutation' && result.method === 'flash';
    case 'build_request':
      return result.kind === 'mutation' && result.method === 'contained_build';
    case 'agent_run_register':
      return result.kind === 'mutation' && result.method === 'agent_run_register';
    case 'agent_run_update':
      return result.kind === 'mutation' && result.method === 'agent_run_update';
    case 'work_intent_acknowledge':
      return result.kind === 'mutation' && result.method === 'work_intent_acknowledge';
    default:
      return false;
  }
}

/** A connected, handshaken IPC client. */
export class NodeIpcClient {
  private readonly socket: Socket;
  private readonly pending = new Map<string, PendingCall>();
  private readonly config: ResolvedConfig;
  private closed = false;

  private constructor(
    socket: Socket,
    config: ResolvedConfig,
    public readonly negotiatedCapabilities: ReadonlySet<string>,
    public readonly sessionId: string,
    public readonly authenticatedPrincipal: unknown,
  ) {
    this.socket = socket;
    this.config = config;
    void this.runReaderLoop();
  }

  /**
   * Loads the configured credential (failing closed *before* the pipe is
   * ever opened — §6 Deliverable 5), opens the pipe (retrying a busy-pipe
   * connect error up to `connectTimeoutMs`), and performs the handshake.
   */
  static async connect(config: NodeIpcClientConfig): Promise<NodeIpcClient> {
    const resolved = resolveConfig(config);

    let credential: Buffer;
    try {
      credential = loadCredential(resolved.credentialKind, resolved.credentialPath);
    } catch (error) {
      throw new CredentialLoadError(error);
    }

    const socket = await openWithRetry(resolved.pipeName, resolved.connectTimeoutMs);

    try {
      const challenge = await readOneHandshakeMessage(socket, resolved.maxFrameBytes);
      if (challenge === null) {
        throw new ConnectionClosedDuringHandshakeError();
      }
      if (challenge.type !== 'Challenge') {
        throw new UnexpectedHandshakeMessageError();
      }

      const hello: WireMessage = {
        type: 'Hello',
        client_protocol_version: resolved.protocolVersion,
        client_capabilities: Array.from(resolved.capabilities),
        client_type: resolved.clientType,
        client_version: resolved.clientVersion,
        nonce: challenge.nonce,
        token: credential.toString('hex'),
      };
      try {
        socket.write(encodeFrame(hello, resolved.maxFrameBytes));
      } finally {
        // Best-effort: zeroes this client's own `Buffer` copy immediately
        // after the `Hello` frame is handed to the socket. V8's own
        // internal string/JSON-serialization buffers created while
        // encoding that one frame are not independently wiped by this
        // code — a real, named, partially-irreducible limitation of the
        // runtime, not silently claimed as a complete guarantee.
        credential.fill(0);
      }

      const ack = await readOneHandshakeMessage(socket, resolved.maxFrameBytes);
      if (ack === null) {
        throw new ConnectionClosedDuringHandshakeError();
      }
      if (ack.type === 'HelloReject') {
        throw new HandshakeRejectedError(ack.reason, ack.server_protocol_version);
      }
      if (ack.type !== 'HelloAck') {
        throw new UnexpectedHandshakeMessageError();
      }

      const legalCapabilities = capabilitiesForVersion(resolved.protocolVersion);
      const negotiated = new Set(ack.negotiated_capabilities);
      for (const capability of negotiated) {
        if (!resolved.capabilities.has(capability) || !legalCapabilities.has(capability)) {
          throw new UnexpectedHandshakeMessageError();
        }
      }

      return new NodeIpcClient(socket, resolved, negotiated, ack.session_id, ack.principal ?? null);
    } catch (error) {
      socket.destroy();
      throw error;
    }
  }

  private async runReaderLoop(): Promise<void> {
    try {
      for await (const message of readFrames(this.socket, this.config.maxFrameBytes)) {
        this.dispatch(message);
      }
    } catch {
      // A frame/connection error ends the reader loop, matching
      // `client.rs::run_reader_loop`'s own `Err(err) => { ...; break; }`.
    }
    this.failAllPending();
  }

  private dispatch(message: WireMessage): void {
    if (message.type === 'CallResult') {
      const waiter = this.pending.get(message.correlation_id);
      this.pending.delete(message.correlation_id);
      if (waiter?.kind === 'unary') {
        waiter.resolve(message.outcome);
      }
      return;
    }
    if (message.type === 'StreamItem') {
      const waiter = this.pending.get(message.correlation_id);
      if (waiter?.kind === 'stream') {
        waiter.push({ type: 'item', item: message.item });
      }
      return;
    }
    if (message.type === 'StreamEnd') {
      const waiter = this.pending.get(message.correlation_id);
      this.pending.delete(message.correlation_id);
      if (waiter?.kind === 'stream') {
        waiter.push({ type: 'ended', outcome: message.outcome });
      }
      return;
    }
    // Any other post-handshake message type is unexpected; ignored rather
    // than treated as fatal, matching `client.rs`'s own `tracing::warn!`
    // (non-fatal) treatment of an unexpected post-handshake message.
  }

  private failAllPending(): void {
    for (const [id, waiter] of this.pending) {
      this.pending.delete(id);
      if (waiter.kind === 'unary') {
        waiter.reject(new ConnectionClosedError());
      }
    }
  }

  /** Calls the `ping` method. `timeoutMs` becomes the call's absolute
   * deadline, primarily enforced by the server. */
  async ping(
    payload: string,
    timeoutMs: number,
  ): Promise<{ payload: string; server_unix_millis: number }> {
    const correlationId = crypto.randomUUID();
    const method: Method = { method: 'Ping', params: { payload } };
    const outcome = await this.callUnary(correlationId, method, timeoutMs);
    if (outcome.outcome === 'err') {
      throw new RpcCallError(outcome.error);
    }
    if (outcome.result.method !== 'Ping') {
      throw new ProtocolMismatchError('expected a Ping result');
    }
    return outcome.result.result;
  }

  /** Calls any business unary method, validating the response's shape
   * against `method` before returning it — never hands a caller a
   * mismatched typed result. `onCorrelationId`, if given, is invoked
   * synchronously with the call's own correlation id the instant it is
   * minted — before the request is even sent — so a caller can pass it to
   * {@link cancelCall} for an in-flight call it wants to cancel early. */
  async businessCall(
    method: BusinessRpcRequest,
    timeoutMs: number,
    onCorrelationId?: (correlationId: string) => void,
  ): Promise<BusinessRpcResult> {
    const correlationId = crypto.randomUUID();
    onCorrelationId?.(correlationId);
    const outcome = await this.callUnary(correlationId, method, timeoutMs);
    if (outcome.outcome === 'err') {
      throw new RpcCallError(outcome.error);
    }
    if (outcome.result.method !== 'Business') {
      throw new ProtocolMismatchError('expected a Business result');
    }
    if (!resultMatchesMethod(method, outcome.result.result)) {
      throw new ProtocolMismatchError(
        `server result kind did not match the requested method ${method.method}`,
      );
    }
    return outcome.result.result;
  }

  private async callUnary(
    correlationId: string,
    method: Method,
    timeoutMs: number,
  ): Promise<CallOutcome> {
    const call: WireMessage = {
      type: 'Call',
      correlation_id: asCorrelationId(correlationId),
      deadline_unix_millis: Date.now() + timeoutMs,
      method,
    };

    return new Promise<CallOutcome>((resolve, reject) => {
      let settled = false;
      const timer = setTimeout(() => {
        if (settled) {
          return;
        }
        settled = true;
        this.pending.delete(correlationId);
        reject(new ClientTimedOutError());
      }, timeoutMs + CLIENT_DEADLINE_GRACE_MS);

      this.pending.set(correlationId, {
        kind: 'unary',
        resolve: (outcome) => {
          if (settled) {
            return;
          }
          settled = true;
          clearTimeout(timer);
          resolve(outcome);
        },
        reject: (error) => {
          if (settled) {
            return;
          }
          settled = true;
          clearTimeout(timer);
          reject(error);
        },
      });

      try {
        this.socket.write(encodeFrame(call, this.config.maxFrameBytes));
      } catch (error) {
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          this.pending.delete(correlationId);
          reject(error instanceof Error ? error : new Error(String(error)));
        }
      }
    });
  }

  /** Calls `business_events`, returning a handle to consume its events. */
  businessStream(
    params: Extract<Method, { method: 'business_events' }>['params'],
    timeoutMs: number,
  ): BusinessStreamHandle {
    const correlationId = crypto.randomUUID();
    const handle = new BusinessStreamHandle(correlationId);
    this.pending.set(correlationId, {
      kind: 'stream',
      push: (event) => handle.push(event),
    });
    const call: WireMessage = {
      type: 'CallStream',
      correlation_id: asCorrelationId(correlationId),
      deadline_unix_millis: Date.now() + timeoutMs,
      method: { method: 'business_events', params },
    };
    this.socket.write(encodeFrame(call, this.config.maxFrameBytes));
    return handle;
  }

  /** Asks the server to stop an in-flight call early. Best-effort and
   * idempotent — fire-and-forget, matching `client.rs::cancel_call`. */
  cancelCall(correlationId: string): void {
    if (this.closed) {
      return;
    }
    const message: WireMessage = {
      type: 'CancelCall',
      correlation_id: asCorrelationId(correlationId),
    };
    try {
      this.socket.write(encodeFrame(message, this.config.maxFrameBytes));
    } catch {
      // Best-effort — a write failure here just means the connection is
      // already gone, which the reader loop will also observe.
    }
  }

  /** Closes the connection. No auto-reconnect exists anywhere in this
   * client — a caller must call `NodeIpcClient.connect(...)` again. */
  close(): void {
    this.closed = true;
    this.socket.destroy();
  }
}

async function readOneHandshakeMessage(
  socket: Socket,
  maxFrameBytes: number,
): Promise<WireMessage | null> {
  const iterator = readFrames(socket, maxFrameBytes)[Symbol.asyncIterator]();
  try {
    const { value, done } = await iterator.next();
    return done ? null : value;
  } catch (error) {
    throw new ConnectFrameError(error instanceof Error ? error.message : String(error));
  }
}

/**
 * Opens the pipe, retrying only a busy-pipe connect error, up to
 * `connectTimeoutMs` — mirrors `client.rs::open_with_retry`'s 20ms poll
 * interval exactly. **No automatic reconnect after an established
 * connection is lost** — this function is only ever called from
 * `NodeIpcClient.connect`, never from inside an already-connected client's
 * read/write path.
 */
async function openWithRetry(pipeName: string, connectTimeoutMs: number): Promise<Socket> {
  const deadline = Date.now() + connectTimeoutMs;
  for (;;) {
    try {
      return await openOnce(pipeName);
    } catch (error) {
      if (isBusyPipeError(error) && Date.now() < deadline) {
        await sleep(20);
        continue;
      }
      if (isBusyPipeError(error)) {
        throw new ConnectTimedOutError(connectTimeoutMs);
      }
      throw new ConnectIoError(
        `failed to open ipc connection: ${error instanceof Error ? error.message : String(error)}`,
        error,
      );
    }
  }
}

function openOnce(pipeName: string): Promise<Socket> {
  return new Promise((resolve, reject) => {
    const socket = net.connect({ path: pipeName });
    const onError = (error: Error): void => {
      socket.removeListener('connect', onConnect);
      reject(error);
    };
    const onConnect = (): void => {
      socket.removeListener('error', onError);
      resolve(socket);
    };
    socket.once('error', onError);
    socket.once('connect', onConnect);
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
