/**
 * DSK-001 — minimal preload script.
 *
 * Exists only to give the renderer a `contextBridge`-exposed surface so
 * `contextIsolation: true` has something concrete to isolate, and so a real
 * preload file is loaded and provable end to end (see
 * `electron/main/security.test.cts`). This is explicitly **not** the real
 * preload API: no `ipcRenderer`, no generic `invoke`, no business logic. The
 * narrow, named-method IPC bridge is `DSK-002`'s scope
 * (`Docs/ARCHITECTURE.md` §4, `AGENTS.md` locked-architecture rule — "no
 * generic `invoke`, raw command, or arbitrary path bridge").
 */
import { contextBridge } from 'electron';

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
