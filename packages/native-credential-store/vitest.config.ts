import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    reporters: 'default',
    // The real cross-language DPAPI round trip talks to a real,
    // `cargo`-built helper process and real Windows DPAPI calls — generous
    // relative to an ordinary unit test, matching this workspace's other
    // real-process integration suites (`Docs/TESTING.md`).
    testTimeout: 30_000,
    // `child_process`/N-API native-addon interaction is more reliably
    // observed running each test file in its own real OS process
    // (`forks`) than in a `worker_threads`-based pool — this suite spawns
    // a real subprocess (`mint_test_credential.exe`) and loads a real
    // compiled native addon per test.
    pool: 'forks',
  },
});
