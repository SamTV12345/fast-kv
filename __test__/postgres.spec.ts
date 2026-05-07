import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('postgres', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('postgres:18-alpine')
      .withExposedPorts(5432)
      .withEnvironment({
        POSTGRES_USER: 'ueberdb',
        POSTGRES_PASSWORD: 'ueberdb',
        POSTGRES_DB: 'ueberdb',
      })
      .start()
    databases.postgres.host = container.getHost()
    databases.postgres.port = container.getMappedPort(5432)
  }, 120_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('postgres')
})
