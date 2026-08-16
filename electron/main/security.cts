/**
 * DSK-001 — Electron security baseline (pure, side-effect-free policy module).
 *
 * Every function here is a pure constructor/predicate with no dependency on a
 * running `app`/`BrowserWindow` instance, specifically so the actual objects
 * `electron/main/index.cts` hands to Electron can be imported and asserted on
 * directly by `security.test.cts` — proving the real `webPreferences` object,
 * not a re-typed copy of it. See `Docs/SECURITY.md` section 5 ("Electron
 * security baseline") for the normative requirements this file implements.
 */
import type {
  HandlerDetails,
  HeadersReceivedResponse,
  OnHeadersReceivedListenerDetails,
  Session,
  WebPreferences,
  WindowOpenHandlerResponse,
} from 'electron';

/**
 * Electron's own handler function types, extracted directly from `Session`
 * rather than hand-reconstructed, so this file cannot silently drift from
 * whatever shape the installed Electron version actually expects.
 */
type PermissionRequestHandler = NonNullable<Parameters<Session['setPermissionRequestHandler']>[0]>;
type PermissionCheckHandler = NonNullable<Parameters<Session['setPermissionCheckHandler']>[0]>;

/**
 * The exact `webPreferences` object passed to `new BrowserWindow(...)`.
 *
 * `Docs/SECURITY.md` release-blocking baseline:
 *   - `nodeIntegration: false` for every renderer
 *   - `contextIsolation: true` for every renderer
 *   - renderer sandbox enabled globally
 *   - no `<webview>` unless an accepted ADR proves the need and isolation
 *   - no remote-code execution surface (no `enableRemoteModule` — that flag no
 *     longer exists in modern Electron; the built-in remote module was removed
 *     entirely. Not depending on the `@electron/remote` package is what "no
 *     remote module" means today, and this file has no such import.)
 */
export function createWebPreferences(preloadPath: string): WebPreferences {
  return {
    preload: preloadPath,
    contextIsolation: true,
    nodeIntegration: false,
    nodeIntegrationInSubFrames: false,
    nodeIntegrationInWorker: false,
    sandbox: true,
    webSecurity: true,
    allowRunningInsecureContent: false,
    webviewTag: false,
    navigateOnDragDrop: false,
    disableBlinkFeatures: 'Auxclick',
  };
}

/**
 * `session.setPermissionRequestHandler` — denies every permission request by
 * default (camera, microphone, geolocation, notifications, HID, USB, serial,
 * clipboard, etc.). DSK-001 grants the renderer no device/OS capability; a
 * later node that needs one specific permission must extend this explicitly
 * and record why, not remove the default deny.
 */
export function createPermissionRequestHandler(): PermissionRequestHandler {
  return (_webContents, _permission, callback, _details): void => {
    callback(false);
  };
}

/**
 * `session.setPermissionCheckHandler` — the synchronous counterpart Electron's
 * docs require alongside `setPermissionRequestHandler` for complete coverage.
 * Also denies by default.
 */
export function createPermissionCheckHandler(): PermissionCheckHandler {
  return (): boolean => false;
}

/**
 * `webContents.setWindowOpenHandler` — denies every `window.open()`/target
 * `_blank` attempt. DSK-001 has no approved external-origin allowlist yet
 * (that is a later node's config input), so the only honest default is deny.
 */
export function createWindowOpenHandler(): (details: HandlerDetails) => WindowOpenHandlerResponse {
  return (): WindowOpenHandlerResponse => ({ action: 'deny' });
}

/**
 * Pure predicate: is `targetUrl` navigation allowed away from
 * `allowedOriginUrl`? Used from a `will-navigate` listener to stop the window
 * from ever leaving the packaged renderer (or, in dev, the Vite dev server
 * origin). Same-document/hash navigation and same-origin navigation are
 * allowed; everything else (including a different scheme, host, or port) is
 * denied. `file:` origins are compared by exact URL prefix since `file:` URLs
 * have no meaningful "origin" the way http(s) URLs do.
 */
export function isNavigationAllowed(targetUrl: string, allowedOriginUrl: string): boolean {
  let target: URL;
  let allowed: URL;
  try {
    target = new URL(targetUrl);
    allowed = new URL(allowedOriginUrl);
  } catch {
    return false;
  }

  if (allowed.protocol === 'file:') {
    // Compare the directory the packaged renderer's index.html lives in, so
    // client-side router navigations to the same index.html stay allowed.
    const allowedDir = allowed.href.slice(0, allowed.href.lastIndexOf('/') + 1);
    return target.protocol === 'file:' && target.href.startsWith(allowedDir);
  }

  return target.protocol === allowed.protocol && target.host === allowed.host;
}

/**
 * Builds a `session.webRequest.onHeadersReceived` listener that injects a
 * `Content-Security-Policy` response header on every response, without
 * requiring any edit to `LokiAI Desktop UI/index.html` (which stays byte-for-
 * byte unchanged — DSK-001 forbids touching `LokiAI Desktop UI/src/**`, and
 * this keeps the wrapper's hands off the renderer package entirely).
 */
export function createCspHeadersListener(
  contentSecurityPolicy: string,
): (
  details: OnHeadersReceivedListenerDetails,
  callback: (response: HeadersReceivedResponse) => void,
) => void {
  return (details, callback): void => {
    const responseHeaders = { ...(details.responseHeaders ?? {}) };
    responseHeaders['Content-Security-Policy'] = [contentSecurityPolicy];
    callback({ cancel: false, responseHeaders });
  };
}

/**
 * Production CSP for the packaged renderer loaded from `file://`.
 *
 * `style-src`/`font-src` allow Google Fonts because the *unmodified*
 * `LokiAI Desktop UI/index.html` links `fonts.googleapis.com` /
 * `fonts.gstatic.com` directly — blocking them would silently change the
 * rendered UI (missing webfonts), which DSK-001's objective explicitly rules
 * out ("zero renderer behavior/visual changes"). No `'unsafe-inline'` and no
 * `'unsafe-eval'`: the production Vite build emits only external
 * `<script type="module" src="...">` tags, and React/Motion set inline styles
 * through the `CSSStyleDeclaration` property setter, not the `style=""`
 * attribute string, so neither needs a script/style relaxation. Verified by
 * inspecting the built `dist/index.html` and by capturing renderer
 * console-message events during the packaged smoke run (see the DSK-001
 * handoff for the captured log).
 */
export const PRODUCTION_CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self' https://fonts.googleapis.com",
  "font-src 'self' https://fonts.gstatic.com",
  "img-src 'self' data:",
  "connect-src 'self'",
  "worker-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-src 'none'",
  "frame-ancestors 'none'",
].join('; ');

/**
 * Development-only CSP applied solely when the window is pointed at the live
 * Vite dev server (`LOKIAI_DEV_SERVER_URL` set — see `rendererTarget.cts`).
 * Vite's dev server injects an inline React-Refresh preamble `<script>` and
 * HMR `<style>` updates, and talks back to itself over a `ws://` connection;
 * none of that ships in a package, so it is intentionally not the production
 * policy above. `electron:package` never sets `LOKIAI_DEV_SERVER_URL`, so a
 * packaged build cannot accidentally load this relaxed policy.
 */
export const DEVELOPMENT_CONTENT_SECURITY_POLICY = [
  "default-src 'self' http://localhost:* ws://localhost:*",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval' http://localhost:*",
  "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com http://localhost:*",
  "font-src 'self' https://fonts.gstatic.com http://localhost:*",
  "img-src 'self' data: http://localhost:*",
  "connect-src 'self' ws://localhost:* http://localhost:*",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join('; ');
