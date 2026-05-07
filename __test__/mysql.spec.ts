import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('mysql', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('mysql:8')
      .withExposedPorts(3306)
      .withEnvironment({
        MYSQL_USER: 'ueberdb',
        MYSQL_PASSWORD: 'ueberdb',
        MYSQL_DATABASE: 'ueberdb',
        MYSQL_ROOT_PASSWORD: 'rootpw',
      })
      .withWaitStrategy(Wait.forLogMessage(/ready for connections.*port: 3306/, 2))
      .withStartupTimeout(120_000)
      .start()
    databases.mysql.host = container.getHost()
    databases.mysql.port = container.getMappedPort(3306)
  }, 180_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('mysql')
})
