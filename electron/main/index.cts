/**
 * DSK-001 — Electron main process entry point.
 *
 * Scope (see `Docs/graph/tasks/DSK-001.md`): wrap the existing, unmodified
 * `LokiAI Desktop UI/` renderer in a packaged, security-hardened Electron
 * shell. No real preload API/business logic (that is `DSK-002`), no
 * auto-update (`PKG-002`), no tray/notifications beyond truthful lifecycle.
 *
 * Compiled by `electron/tsconfig.json` to `electron/dist/main/index.cjs`
 * (CommonJS, via the `.cts` source extension, regardless of the repository
 * root `package.json`'s `"type": "module"` — see that tsconfig's header
 * comment for why).
 */
import { BrowserWindow, app, session } from 'electron';
import path from 'node:path';
import { createLogger } from './log.cjs';
import { buildMissingRendererHtml, resolveRendererTarget } from './rendererTarget.cjs';
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

const logger = createLogger();

let mainWindow: BrowserWindow | null = null;

function focusMainWindow(): void {
  if (!mainWindow) {
    return;
  }
  if (mainWindow.isMinimized()) {
    mainWindow.restore();
  }
  mainWindow.focus();
}

/**
 * Session-wide hardening that must be in place before any content loads:
 * deny every permission request/check by default. `Docs/SECURITY.md` §5.
 */
function applySessionHardening(): void {
  session.defaultSession.setPermissionRequestHandler(createPermissionRequestHandler());
  session.defaultSession.setPermissionCheckHandler(createPermissionCheckHandler());
}

async function createMainWindow(): Promise<BrowserWindow> {
  const preloadPath = path.join(__dirname, '..', 'preload', 'index.cjs');
  const target = resolveRendererTarget(
    __dirname,
    process.env,
    PRODUCTION_CONTENT_SECURITY_POLICY,
    DEVELOPMENT_CONTENT_SECURITY_POLICY,
  );

  // Scoped to the default session, which is the only session this app's
  // single BrowserWindow uses (no custom `partition`).
  session.defaultSession.webRequest.onHeadersReceived(
    createCspHeadersListener(target.contentSecurityPolicy),
  );

  const win = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1024,
    minHeight: 640,
    show: false,
    autoHideMenuBar: true,
    backgroundColor: '#010101',
    webPreferences: createWebPreferences(preloadPath),
  });
  mainWindow = win;

  win.webContents.setWindowOpenHandler(createWindowOpenHandler());

  const allowedOriginUrl =
    target.kind === 'dev-server'
      ? (target.url ?? '')
      : `file://${(target.filePath ?? '').replace(/\\/g, '/')}`;

  win.webContents.on('will-navigate', (details) => {
    if (!isNavigationAllowed(details.url, allowedOriginUrl)) {
      logger.warn('navigation-blocked', { url: details.url });
      details.preventDefault();
    }
  });

  win.webContents.on('console-message', (details) => {
    logger.info('renderer-console-message', {
      level: details.level,
      message: details.message,
      line: details.lineNumber,
      source: details.sourceId,
    });
  });

  win.webContents.on('did-finish-load', () => {
    logger.info('renderer-did-finish-load', { kind: target.kind });
  });

  win.webContents.on('did-fail-load', (_event, errorCode, errorDescription, validatedURL) => {
    logger.error('renderer-did-fail-load', { errorCode, errorDescription, validatedURL });
  });

  win.webContents.on('render-process-gone', (_event, details) => {
    logger.error('renderer-process-gone', { reason: details.reason, exitCode: details.exitCode });
  });

  win.once('ready-to-show', () => {
    win.show();
    logger.info('window-shown');
  });

  win.on('closed', () => {
    if (mainWindow === win) {
      mainWindow = null;
    }
  });

  switch (target.kind) {
    case 'dev-server':
      logger.info('loading-dev-server', { url: target.url });
      await win.loadURL(target.url ?? '');
      break;
    case 'built-file':
      logger.info('loading-built-file', { filePath: target.filePath });
      await win.loadFile(target.filePath ?? '');
      break;
    case 'missing-build':
      logger.error('renderer-build-missing', { expectedPath: target.filePath });
      await win.loadURL(
        `data:text/html;charset=utf-8,${encodeURIComponent(buildMissingRendererHtml(target.filePath ?? ''))}`,
      );
      break;
  }

  return win;
}

async function main(): Promise<void> {
  await app.whenReady();
  logger.info('app-ready', {
    appVersion: app.getVersion(),
    electronVersion: process.versions.electron,
    chromeVersion: process.versions.chrome,
    nodeVersion: process.versions.node,
  });

  applySessionHardening();
  await createMainWindow();

  app.on('activate', () => {
    // Windows/Linux baseline (TECH_STACK.md §2): this app does not target
    // macOS, but re-creating the window on `activate` with no windows open is
    // a harmless, standard Electron lifecycle guard to keep regardless.
    if (BrowserWindow.getAllWindows().length === 0) {
      void createMainWindow();
    }
  });
}

const gotSingleInstanceLock = app.requestSingleInstanceLock();

if (!gotSingleInstanceLock) {
  logger.info('second-instance-blocked-self', { pid: process.pid });
  app.quit();
} else {
  app.on('second-instance', (_event, argv, workingDirectory) => {
    logger.info('second-instance-focus-requested', { argv, workingDirectory });
    focusMainWindow();
  });

  app.on('window-all-closed', () => {
    logger.info('window-all-closed');
    app.quit();
  });

  app.on('before-quit', () => logger.info('before-quit'));
  app.on('will-quit', () => logger.info('will-quit'));
  app.on('quit', (_event, exitCode) => logger.info('quit', { exitCode }));

  main().catch((error: unknown) => {
    logger.error('main-failed', { error: error instanceof Error ? error.stack : String(error) });
    app.exit(1);
  });
}
