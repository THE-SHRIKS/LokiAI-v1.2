/**
 * `P1-NODE-IPC-CLIENT-001`: typed client-side error taxonomy, mirroring
 * `crates/loki-ipc/src/client.rs`'s `ConnectError`/`CallError` enums —
 * every rejection reason is a specific, named, distinguishable class, never
 * a bare thrown string.
 */

import type { HandshakeRejectReason, ProtocolVersion, RpcError } from '@lokiai/contracts';

export abstract class NodeIpcClientError extends Error {
  constructor(message: string) {
    super(message);
    this.name = new.target.name;
  }
}

// --- Connect-time errors (mirrors `ConnectError`) ---------------------------------

export class ConnectIoError extends NodeIpcClientError {
  constructor(
    message: string,
    public override readonly cause?: unknown,
  ) {
    super(message);
  }
}

export class ConnectFrameError extends NodeIpcClientError {}

export class ConnectionClosedDuringHandshakeError extends NodeIpcClientError {
  constructor() {
    super('server closed the connection during handshake');
  }
}

export class UnexpectedHandshakeMessageError extends NodeIpcClientError {
  constructor() {
    super('server sent an unexpected message during handshake');
  }
}

/** Mirrors `ConnectError::Rejected`. */
export class HandshakeRejectedError extends NodeIpcClientError {
  constructor(
    public readonly reason: HandshakeRejectReason,
    public readonly serverProtocolVersion: ProtocolVersion,
  ) {
    super(
      `handshake rejected by server (protocol ${serverProtocolVersion.major}.${serverProtocolVersion.minor}): ${reason.reason}`,
    );
  }
}

/** The connect attempt's own busy-pipe retry loop exceeded `connectTimeoutMs`. */
export class ConnectTimedOutError extends NodeIpcClientError {
  constructor(connectTimeoutMs: number) {
    super(
      `could not open the pipe within connectTimeoutMs (${connectTimeoutMs}ms), pipe stayed busy`,
    );
  }
}

// --- Call-time errors (mirrors `CallError`) ---------------------------------------

/** The connection was lost before a response arrived. */
export class ConnectionClosedError extends NodeIpcClientError {
  constructor() {
    super('connection closed before a response arrived');
  }
}

/** Neither a response nor the server's own `DeadlineExceeded` arrived even
 * after the client-side deadline grace period elapsed. Distinct from a
 * server-acknowledged `RpcCallError` carrying `RpcError::DeadlineExceeded`. */
export class ClientTimedOutError extends NodeIpcClientError {
  constructor() {
    super('no response from server within the deadline grace period');
  }
}

/** The server returned a named RPC failure (including a normal,
 * server-acknowledged `DeadlineExceeded`/`Cancelled`). */
export class RpcCallError extends NodeIpcClientError {
  constructor(public readonly rpcError: RpcError) {
    super(`server returned an RPC error: ${rpcError.kind}`);
  }
}

/** The server's response did not have the shape this method expects. */
export class ProtocolMismatchError extends NodeIpcClientError {
  constructor(detail: string) {
    super(`server returned a mismatched typed result: ${detail}`);
  }
}

/** The credential this client was configured to present could not be
 * loaded (missing/corrupt/wrong-kind) — surfaced *before* the pipe is ever
 * opened, per `P1-NODE-IPC-CLIENT-001` §6 Deliverable 5's fail-closed
 * requirement. Wraps the specific typed error
 * `@lokiai/native-credential-store` threw. */
export class CredentialLoadError extends NodeIpcClientError {
  constructor(public override readonly cause: unknown) {
    super(
      `failed to load the configured credential: ${(cause as Error)?.message ?? String(cause)}`,
    );
  }
}

/** The session's credential was found revoked at revalidation time — a
 * distinguishable, terminal authentication failure, never retried, never
 * misreported as a generic connection error. Mirrors
 * `RpcError::CredentialRevalidationFailed`. */
export class CredentialRevalidationFailedError extends RpcCallError {}
