import { defineConfig } from 'vitest/config';

/**
 * The integration tests spin up an in-process PGlite database per test; the first
 * one in each file pays a one-off WASM cold-start (~6 s), so the default 5 s budget
 * is too tight. The load test sets its own longer timeout inline.
 */
export default defineConfig({
  test: {
    testTimeout: 20_000,
    hookTimeout: 20_000,
  },
});
