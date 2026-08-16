/**
 * DSK-001 — minimal structured lifecycle logger.
 *
 * This is deliberately small: real diagnostics/log persistence is a later
 * node's job (see `Docs/ARCHITECTURE.md` bounded context 10, "Diagnostics and
 * Updates"). What DSK-001 needs is *truthful, provable* lifecycle events
 * (app ready, window created, second-instance blocked, quit) instead of
 * narrated claims. Every event is a single JSON line on stdout; if
 * `LOKIAI_LOG_FILE` is set (used only for capturing verification evidence
 * during this task, never required for normal operation) the same line is
 * also appended to that file.
 */
import { appendFileSync } from 'node:fs';

export interface Logger {
  info(event: string, data?: Record<string, unknown>): void;
  warn(event: string, data?: Record<string, unknown>): void;
  error(event: string, data?: Record<string, unknown>): void;
}

function writeLine(
  level: 'info' | 'warn' | 'error',
  event: string,
  data: Record<string, unknown> | undefined,
  logFilePath: string | undefined,
): void {
  const line = JSON.stringify({
    ts: new Date().toISOString(),
    level,
    event,
    pid: process.pid,
    ...(data ?? {}),
  });
  process.stdout.write(`${line}\n`);
  if (logFilePath) {
    try {
      appendFileSync(logFilePath, `${line}\n`, 'utf8');
    } catch {
      // Logging must never crash the app; a failed evidence-file write is
      // silently dropped from the file but still reached stdout above.
    }
  }
}

export function createLogger(env: NodeJS.ProcessEnv = process.env): Logger {
  const logFilePath = env.LOKIAI_LOG_FILE;
  return {
    info: (event, data): void => writeLine('info', event, data, logFilePath),
    warn: (event, data): void => writeLine('warn', event, data, logFilePath),
    error: (event, data): void => writeLine('error', event, data, logFilePath),
  };
}
