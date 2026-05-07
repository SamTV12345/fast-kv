import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    hookTimeout: 60000,
    // Some backends (redb, dirty append-log) fsync every write, so the
    // 1000-op speed benchmark needs more headroom than the 5s default.
    testTimeout: 60000,
  },
})
