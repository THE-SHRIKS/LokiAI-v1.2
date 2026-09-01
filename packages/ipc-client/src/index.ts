/**
 * `@lokiai/ipc-client` — `P1-NODE-IPC-CLIENT-001`.
 *
 * The first real, reusable Node.js named-pipe client for `loki-ipc`'s
 * business RPC protocol — wire-compatible byte-for-byte with
 * `crates/loki-ipc/src/client.rs`. See `Docs/graph/tasks/
 * P1-NODE-IPC-CLIENT-001.md` and `Docs/graph/decisions/ADR-0009-node-ipc-
 * credential-distribution.md`.
 *
 * Library only: no Electron main, MCP server, or renderer wiring exists
 * here (later, separate nodes' scope).
 */

export {
  BusinessStreamHandle,
  CLIENT_DEADLINE_GRACE_MS,
  NodeIpcClient,
  type NodeIpcClientConfig,
  type StreamEvent,
} from './client.js';
export {
  DEFAULT_MAX_FRAME_BYTES,
  FrameMalformedError,
  FrameTooLargeError,
  FrameTruncatedError,
  encodeFrame,
  readFrames,
} from './frame.js';
export {
  ClientTimedOutError,
  ConnectFrameError,
  ConnectIoError,
  ConnectTimedOutError,
  ConnectionClosedDuringHandshakeError,
  ConnectionClosedError,
  CredentialLoadError,
  CredentialRevalidationFailedError,
  HandshakeRejectedError,
  NodeIpcClientError,
  ProtocolMismatchError,
  RpcCallError,
  UnexpectedHandshakeMessageError,
} from './errors.js';
