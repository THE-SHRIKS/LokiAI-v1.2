/**
 * DSK-001 — security-baseline tests.
 *
 * These import the exact functions `electron/main/index.cts` calls to build
 * the real `BrowserWindow`/`session` configuration, and assert on the actual
 * returned objects/behaviour — not a re-typed copy of what the code "should"
 * do. Run via `pnpm run electron:test` (compiles this file to
 * `electron/dist/main/security.test.cjs`, then `node --test electron/dist`).
 */
import assert from 'node:assert/strict';
import { test } from 'node:test';
import fs from 'node:fs';
import path from 'node:path';
import type {
  PermissionCheckHandlerHandlerDetails,
  PermissionRequest,
  WebContents,
} from 'electron';
import {
  DEVELOPMENT_CONTENT_SECURITY_POLICY,
  PRODUCTION_CONTENT_SECURITY_POLICY,
  createCspHeadersListener,
  createPermissionCheckHandler,
  createPermissionRequestHandler,
  createWebPreferences,
  createWindowOpenHandler,
  isNavigationAllowed,
} from './security.cjs';

void test('createWebPreferences sets the full Docs/SECURITY.md §5 renderer baseline', () => {
  const prefs = createWebPreferences('C:\\app\\preload\\index.cjs');

  // The exact three settings DSK-001's acceptance criteria names.
  assert.equal(prefs.contextIsolation, true, 'contextIsolation must be true');
  assert.equal(prefs.nodeIntegration, false, 'nodeIntegration must be false');
  assert.equal(prefs.sandbox, true, 'sandbox must be true');

  // Defense-in-depth settings the same baseline requires.
  assert.equal(prefs.nodeIntegrationInSubFrames, false);
  assert.equal(prefs.nodeIntegrationInWorker, false);
  assert.equal(prefs.webSecurity, true);
  assert.equal(prefs.allowRunningInsecureContent, false);
  assert.equal(prefs.webviewTag, false, 'no <webview> without an accepted ADR');
  assert.equal(prefs.navigateOnDragDrop, false);
  assert.equal(prefs.preload, 'C:\\app\\preload\\index.cjs');
});

void test('createWebPreferences does not silently vary by call site (same input -> same output)', () => {
  const a = createWebPreferences('/x/preload.cjs');
  const b = createWebPreferences('/x/preload.cjs');
  assert.deepEqual(a, b);
});

void test('the real Electron package has no remote module installed', () => {
  // Structural proof, not a grep: if `@electron/remote` (or the removed
  // built-in) were ever added as a dependency, `require.resolve` would
  // succeed and this test would fail. `Docs/SECURITY.md` requires "no
  // remote module" as a release-blocking control.
  assert.throws(
    () => require.resolve('@electron/remote'),
    /Cannot find module/,
    '@electron/remote must not be an installed dependency',
  );
});

/**
 * Strips `/* ... *\/` and `// ...` comments so this check looks at real code,
 * not the doc comments that (deliberately) *describe* what is withheld —
 * e.g. preload/index.cts's own header explains it exposes no `ipcRenderer`,
 * which would otherwise false-positive a naive substring search.
 */
function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '');
}

void test('main/preload source does not reference the remote module or ipcRenderer', () => {
  const repoRoot = process.cwd();
  const filesToScan = [
    path.join(repoRoot, 'electron', 'main', 'index.cts'),
    path.join(repoRoot, 'electron', 'preload', 'index.cts'),
  ];
  for (const filePath of filesToScan) {
    const code = stripComments(fs.readFileSync(filePath, 'utf8'));
    assert.ok(!code.includes('@electron/remote'), `${filePath} must not import the remote module`);
    assert.ok(!code.includes('ipcRenderer'), `${filePath} must not touch ipcRenderer directly`);
  }
});

void test('createPermissionRequestHandler denies every permission by default', () => {
  const handler = createPermissionRequestHandler();
  const permissions: string[] = [
    'clipboard-read',
    'geolocation',
    'media',
    'midi',
    'notifications',
    'openExternal',
    'hid',
    'usb',
    'serial',
  ];
  for (const permission of permissions) {
    let grantedValue: boolean | undefined;
    handler(
      undefined as unknown as WebContents,
      permission as never,
      (granted: boolean) => {
        grantedValue = granted;
      },
      {} as PermissionRequest,
    );
    assert.equal(grantedValue, false, `permission "${permission}" must be denied`);
  }
});

void test('createPermissionCheckHandler denies synchronously by default', () => {
  const handler = createPermissionCheckHandler();
  const result = handler(
    null,
    'geolocation',
    'https://example.test',
    {} as PermissionCheckHandlerHandlerDetails,
  );
  assert.equal(result, false);
});

void test('createWindowOpenHandler denies every window.open()/target=_blank attempt', () => {
  const handler = createWindowOpenHandler();
  const response = handler({
    url: 'https://example.test/',
    frameName: '',
    features: '',
    disposition: 'foreground-tab',
    referrer: { policy: 'no-referrer', url: '' },
  });
  assert.deepEqual(response, { action: 'deny' });
});

void test('isNavigationAllowed: same-origin http(s) navigation is allowed', () => {
  assert.equal(
    isNavigationAllowed('http://localhost:3000/settings', 'http://localhost:3000/'),
    true,
  );
});

void test('isNavigationAllowed: a different host is denied', () => {
  assert.equal(isNavigationAllowed('https://evil.example/', 'http://localhost:3000/'), false);
});

void test('isNavigationAllowed: a different scheme is denied', () => {
  assert.equal(isNavigationAllowed('file:///etc/passwd', 'http://localhost:3000/'), false);
});

void test('isNavigationAllowed: a different port is denied', () => {
  assert.equal(isNavigationAllowed('http://localhost:4000/', 'http://localhost:3000/'), false);
});

void test('isNavigationAllowed: same packaged renderer directory is allowed for file: targets', () => {
  const allowed = 'file:///C:/app/LokiAI%20Desktop%20UI/dist/index.html';
  const target = 'file:///C:/app/LokiAI%20Desktop%20UI/dist/index.html';
  assert.equal(isNavigationAllowed(target, allowed), true);
});

void test('isNavigationAllowed: a file: target outside the renderer directory is denied', () => {
  const allowed = 'file:///C:/app/LokiAI%20Desktop%20UI/dist/index.html';
  const target = 'file:///C:/Windows/System32/notepad.exe';
  assert.equal(isNavigationAllowed(target, allowed), false);
});

void test('isNavigationAllowed: malformed URLs are denied, not thrown', () => {
  assert.equal(isNavigationAllowed('not a url', 'http://localhost:3000/'), false);
  assert.equal(isNavigationAllowed('http://localhost:3000/', 'not a url'), false);
});

void test('production CSP starts with default-src self and has no unsafe script/eval', () => {
  assert.ok(PRODUCTION_CONTENT_SECURITY_POLICY.startsWith("default-src 'self'"));
  assert.ok(!PRODUCTION_CONTENT_SECURITY_POLICY.includes('unsafe-inline'));
  assert.ok(!PRODUCTION_CONTENT_SECURITY_POLICY.includes('unsafe-eval'));
  assert.match(PRODUCTION_CONTENT_SECURITY_POLICY, /script-src 'self'(;|$)/);
  assert.ok(PRODUCTION_CONTENT_SECURITY_POLICY.includes("object-src 'none'"));
  assert.ok(PRODUCTION_CONTENT_SECURITY_POLICY.includes("frame-ancestors 'none'"));
});

void test('development CSP is a distinct, more relaxed policy (never used for a packaged build)', () => {
  assert.notEqual(DEVELOPMENT_CONTENT_SECURITY_POLICY, PRODUCTION_CONTENT_SECURITY_POLICY);
  assert.ok(DEVELOPMENT_CONTENT_SECURITY_POLICY.includes('unsafe-inline'));
});

void test('createCspHeadersListener injects the Content-Security-Policy response header', () => {
  const listener = createCspHeadersListener("default-src 'self'");
  let received: { responseHeaders?: Record<string, string[] | string> } | undefined;
  listener(
    {
      id: 1,
      url: 'file:///C:/app/index.html',
      method: 'GET',
      resourceType: 'mainFrame',
      referrer: '',
      timestamp: 0,
      statusLine: 'HTTP/1.1 200 OK',
      statusCode: 200,
      responseHeaders: { 'x-existing': ['keep-me'] },
    },
    (response) => {
      received = response;
    },
  );
  assert.ok(received);
  assert.deepEqual(received?.responseHeaders?.['Content-Security-Policy'], ["default-src 'self'"]);
  assert.deepEqual(
    received?.responseHeaders?.['x-existing'],
    ['keep-me'],
    'must not drop unrelated headers',
  );
});
