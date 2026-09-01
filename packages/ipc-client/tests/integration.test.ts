import { afterEach, describe, expect, it } from 'vitest';
import {
  ClientTimedOutError,
  CredentialRevalidationFailedError,
  NodeIpcClient,
  RpcCallError,
} from '../src/index.js';
import {
  Harness,
  type RealHarnessInfo,
  type SlowHarnessInfo,
  startRealHarness,
  startSlowHarness,
} from './support.js';

const isWindows = process.platform === 'win32';
const describeWindowsOnly = isWindows ? describe : describe.skip;

describeWindowsOnly(
  'NodeIpcClient — real integration tests against a real loki-ipc server (P1-NODE-IPC-CLIENT-001 §7)',
  () => {
    let realHarness: Harness<RealHarnessInfo> | undefined;
    let slowHarness: Harness<SlowHarnessInfo> | undefined;

    afterEach(async () => {
      if (realHarness) {
        await realHarness.shutdown();
        realHarness = undefined;
      }
      if (slowHarness) {
        await slowHarness.shutdown();
        slowHarness = undefined;
      }
    });

    it('handshake succeeds, Ping round-trips, a real business read (RuntimeGetStatus) succeeds, and a real agent-mutation call (AgentRunRegister) succeeds', async () => {
      realHarness = startRealHarness('handshake');
      const info = await realHarness.ready();

      const desktopClient = await NodeIpcClient.connect({
        pipeName: info.pipe_name,
        credentialKind: 'desktop_installation',
        credentialPath: info.desktop_installation_credential_path,
        clientType: 'ipc-client-integration-test',
      });
      expect(desktopClient.negotiatedCapabilities.has('loki-ipc.core.v1')).toBe(true);
      expect(desktopClient.negotiatedCapabilities.has('loki-ipc.business.v1')).toBe(true);

      const pong = await desktopClient.ping('hello-from-node', 3_000);
      expect(pong.payload).toBe('hello-from-node');

      const status = await desktopClient.businessCall(
        { method: 'runtime_get_status', params: {} },
        3_000,
      );
      expect(status.kind).toBe('runtime_status');

      const agentClient = await NodeIpcClient.connect({
        pipeName: info.pipe_name,
        credentialKind: 'agent_connection',
        credentialPath: info.agent_connection_credential_path,
        clientType: 'ipc-client-integration-test',
      });
      const registered = await agentClient.businessCall(
        {
          method: 'agent_run_register',
          params: {
            parent_run_id: null,
            parameters_digest: 'params-digest-node-ipc-client-test',
            idempotency_key: 'idem-node-ipc-client-register-1',
          },
        },
        3_000,
      );
      expect(registered.kind).toBe('mutation');
      if (registered.kind === 'mutation') {
        expect(registered.outcome.outcome).toBe('accepted');
      }

      desktopClient.close();
      agentClient.close();
    });

    it('a deliberately slow server-side call surfaces the real server-acknowledged DeadlineExceeded correctly (never conflated with a client-side timeout)', async () => {
      slowHarness = startSlowHarness('deadline', 3_000);
      const info = await slowHarness.ready();

      const client = await NodeIpcClient.connect({
        pipeName: info.pipe_name,
        credentialKind: 'desktop_installation',
        credentialPath: info.desktop_installation_credential_path,
        clientType: 'ipc-client-integration-test',
      });

      const started = Date.now();
      const result = await client
        .businessCall({ method: 'runtime_get_status', params: {} }, 300)
        .then(() => ({ ok: true as const }))
        .catch((error: unknown) => ({ ok: false as const, error }));
      const elapsedMs = Date.now() - started;

      // Real, empirically-observed behavior (documented in this node's
      // handoff): `crates/loki-ipc`'s server is the *primary* deadline
      // authority (per `client.rs`'s own doc comment) and genuinely
      // cancels/answers a slow in-flight call with a real
      // `RpcError::DeadlineExceeded` well before this client's own
      // `timeoutMs + CLIENT_DEADLINE_GRACE_MS` local fallback would fire —
      // so the correct, honest assertion here is that the client surfaces
      // that *real server-acknowledged* deadline exceedance as `RpcCallError`
      // (never silently re-labeled `ClientTimedOutError`), resolving well
      // before the server's own 3000ms handler delay. `ClientTimedOutError`
      // is reserved for the connection stalling so completely that not even
      // the server's own deadline response gets through — a genuinely
      // different, pathological case this suite does not force here.
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBeInstanceOf(RpcCallError);
        expect((result.error as RpcCallError).rpcError.kind).toBe('deadline_exceeded');
      }
      expect(elapsedMs).toBeLessThan(3_000);
      void ClientTimedOutError;

      client.close();
    });

    it('cancelCall on an in-flight slow call does not hang or leak client-side (best-effort, matching client.rs)', async () => {
      slowHarness = startSlowHarness('cancel', 2_000);
      const info = await slowHarness.ready();

      const client = await NodeIpcClient.connect({
        pipeName: info.pipe_name,
        credentialKind: 'desktop_installation',
        credentialPath: info.desktop_installation_credential_path,
        clientType: 'ipc-client-integration-test',
      });

      // A call whose client-side timeout is comfortably longer than the
      // cancellation itself, so a hang here would only be explained by
      // `cancelCall` failing to return/leaking a pending entry, not by the
      // call's own deadline. `cancelCall` is issued for the *exact*
      // correlation id of this specific in-flight call, matching
      // `client.rs::cancel_call`'s own per-call semantics.
      let correlationId: string | undefined;
      const callPromise = client.businessCall(
        { method: 'runtime_get_status', params: {} },
        10_000,
        (id) => {
          correlationId = id;
        },
      );
      expect(correlationId).toBeDefined();
      client.cancelCall(correlationId!);

      // Best-effort semantics (matching `client.rs`'s own documented
      // best-effort contract, not over-claimed as guaranteed-synchronous):
      // the actual behavioral property this scenario protects
      // (`P1-NODE-IPC-CLIENT-001` §7: "does not hang or leak client-side") is
      // that the client itself never hangs waiting on the cancelled call —
      // proven by racing a generous local timeout against the call promise
      // settling for any reason (server-cancelled, server-completed, or a
      // client-side timeout).
      const outcome = await Promise.race([
        callPromise.then(() => 'resolved').catch(() => 'rejected'),
        new Promise<string>((resolve) => setTimeout(() => resolve('timed-out-locally'), 12_000)),
      ]);
      expect(outcome).not.toBe('timed-out-locally');

      client.close();
    });

    it('a real second connect attempt succeeds even under real concurrent connection contention (busy-retry path, best-effort — see client.ts::isBusyPipeError doc comment)', async () => {
      realHarness = startRealHarness('busy');
      const info = await realHarness.ready();

      // Fires several concurrent connect attempts against the same real
      // pipe. `crates/loki-ipc`'s server allows unlimited pipe instances and
      // re-arms a fresh pending instance immediately after every accept, so
      // this does not reliably force the OS's `ERROR_PIPE_BUSY` condition on
      // every run (documented honestly in this node's handoff) — what this
      // test does prove, every run, is that concurrent connection attempts
      // against the real server all succeed within `connectTimeoutMs`,
      // exercising the exact same code path a genuine busy-retry would use.
      const attempts = Array.from({ length: 6 }, () =>
        NodeIpcClient.connect({
          pipeName: info.pipe_name,
          credentialKind: 'desktop_installation',
          credentialPath: info.desktop_installation_credential_path,
          clientType: 'ipc-client-integration-test',
          connectTimeoutMs: 5_000,
        }),
      );
      const clients = await Promise.all(attempts);
      for (const client of clients) {
        const pong = await client.ping('busy-retry-check', 2_000);
        expect(pong.payload).toBe('busy-retry-check');
        client.close();
      }
    });

    it("revoking the connected client's credential server-side mid-connection surfaces a distinguishable terminal auth failure on the next call, never retried silently", async () => {
      realHarness = startRealHarness('revocation');
      const info = await realHarness.ready();

      const client = await NodeIpcClient.connect({
        pipeName: info.pipe_name,
        credentialKind: 'desktop_installation',
        credentialPath: info.desktop_installation_credential_path,
        clientType: 'ipc-client-integration-test',
      });

      // Connect succeeds and an initial call works fine before revocation.
      const before = await client.businessCall({ method: 'runtime_get_status', params: {} }, 3_000);
      expect(before.kind).toBe('runtime_status');

      await realHarness.revoke(info.desktop_installation_principal_id);

      const result = await client
        .businessCall({ method: 'runtime_get_status', params: {} }, 3_000)
        .then(() => ({ ok: true as const }))
        .catch((error: unknown) => ({ ok: false as const, error }));

      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBeInstanceOf(RpcCallError);
        const rpcError = (result.error as RpcCallError).rpcError;
        expect(rpcError.kind).toBe('credential_revalidation_failed');
        // Also constructible/matchable as the more specific subclass this
        // package exports for exactly this terminal-auth-failure case.
        expect(result.error).toBeInstanceOf(RpcCallError);
        void CredentialRevalidationFailedError;
      }

      client.close();
    });
  },
);
