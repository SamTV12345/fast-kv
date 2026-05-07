import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('mongodb', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('mongo:7')
      .withExposedPorts(27017)
      .withWaitStrategy(Wait.forLogMessage(/Waiting for connections/))
      .start()
    const host = container.getHost()
    const port = container.getMappedPort(27017)
    databases.mongodb.url = `mongodb://${host}:${port}`
  }, 180_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('mongodb')
})
