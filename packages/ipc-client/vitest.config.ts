import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    reporters: 'default',
    // Real named-pipe handshakes, real deadline/cancellation timing, and a
    // real spawned Rust server process — generous relative to an ordinary
    // unit test, matching `Docs/TESTING.md`'s real-process integration
    // suite convention.
    testTimeout: 30_000,
    pool: 'forks',
  },
});
