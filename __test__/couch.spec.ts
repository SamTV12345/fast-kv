import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('couch', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('couchdb:3')
      .withExposedPorts(5984)
      .withEnvironment({
        COUCHDB_USER: 'ueberdb',
        COUCHDB_PASSWORD: 'ueberdb',
      })
      .withWaitStrategy(Wait.forHttp('/_up', 5984))
      .withStartupTimeout(120_000)
      .start()
    databases.couch.host = container.getHost()
    databases.couch.port = container.getMappedPort(5984)
  }, 180_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('couch')
})
