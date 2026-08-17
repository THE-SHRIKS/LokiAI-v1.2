/**
 * DSK-001 (`lokiaiShell`) + DSK-002 (`loki`) — Electron preload script.
 *
 * `lokiaiShell` is DSK-001's original minimal placeholder, kept byte-for-byte
 * behavior-identical here (extended, not replaced — `Docs/graph/tasks/
 * DSK-002.md` Section 4): it exists only to give the renderer a
 * `contextBridge`-exposed surface so `contextIsolation: true` has something
 * concrete to isolate, and so a real preload file is loaded and provable end
 * to end (see `electron/main/security.test.cts`).
 *
 * `loki` is `DSK-002`'s real, narrow, typed `contextBridge` API: an explicit
 * allowlist of named, validated methods (`electron/preload/bridge.cts`) plus
 * a validated event-subscription mechanism — never a generic `invoke`/raw-
 * command bridge (`Docs/ARCHITECTURE.md` §4, `AGENTS.md` locked-architecture
 * rule). This file only wires the real `electron` module's `ipcRenderer` into
 * the pure, independently-testable `createLokiBridge` factory; it contains no
 * business logic of its own (that lives in `bridge.cts`/`contract.cts`, kept
 * unit-testable without a live Electron renderer process).
 */
import { contextBridge, ipcRenderer } from 'electron';
import { createLokiBridge } from './bridge.cjs';

export interface LokiAiShellInfo {
  readonly electron: string;
  readonly chrome: string;
  readonly node: string;
}

export function buildShellInfo(versions: NodeJS.ProcessVersions): LokiAiShellInfo {
  return Object.freeze({
    electron: versions.electron,
    chrome: versions.chrome,
    node: versions.node,
  });
}

contextBridge.exposeInMainWorld('lokiaiShell', buildShellInfo(process.versions));
contextBridge.exposeInMainWorld('loki', createLokiBridge(ipcRenderer));
