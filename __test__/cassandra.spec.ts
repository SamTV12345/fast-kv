import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('cassandra', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('cassandra:4')
      .withExposedPorts(9042)
      .withEnvironment({
        CASSANDRA_CLUSTER_NAME: 'TestCluster',
        CASSANDRA_DC: 'datacenter1',
        HEAP_NEWSIZE: '128M',
        MAX_HEAP_SIZE: '512M',
      })
      .withWaitStrategy(Wait.forLogMessage(/Created default superuser role|Starting listening for CQL clients/, 1))
      .withStartupTimeout(300_000)
      .start()
    const host = container.getHost()
    const port = container.getMappedPort(9042)
    databases.cassandra.host = host
    databases.cassandra.port = port
    ;(databases.cassandra.clientOptions as any).contactPoints = [`${host}:${port}`]
  }, 360_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('cassandra')
})
