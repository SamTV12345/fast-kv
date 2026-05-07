import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    hookTimeout: 60000,
    // Some backends (redb, dirty append-log) fsync every write, so the
    // 1000-op speed benchmark needs more headroom than the 5s default.
    testTimeout: 60000,
    // Run spec files one at a time. The networked backend specs each
    // spin up a docker container in beforeAll, and running 8+ containers
    // concurrently starves the CPU and makes the per-op speed thresholds
    // flake. The conformance assertions themselves are perfectly happy
    // in parallel — only the benchmark-style 'speed is acceptable' test
    // suffers.
    fileParallelism: false,
  },
})
