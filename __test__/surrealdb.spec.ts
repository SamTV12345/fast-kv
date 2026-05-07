import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('surrealdb', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('surrealdb/surrealdb:v2')
      .withCommand([
        'start',
        '--user',
        'root',
        '--pass',
        'root',
        '--bind',
        '0.0.0.0:8000',
        'memory',
      ])
      .withExposedPorts(8000)
      .withWaitStrategy(Wait.forHttp('/health', 8000))
      .withStartupTimeout(120_000)
      .start()
    databases.surrealdb.host = container.getHost()
    databases.surrealdb.port = container.getMappedPort(8000)
  }, 180_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('surrealdb')
})
