import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('redis', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('redis:7-alpine')
      .withExposedPorts(6379)
      .start()
    const host = container.getHost()
    const port = container.getMappedPort(6379)
    databases.redis.url = `redis://${host}:${port}/`
  }, 120_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('redis')
})
