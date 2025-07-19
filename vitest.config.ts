import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
      hookTimeout: 60000 // Increase timeout for tests to 60 seconds
    }
  })