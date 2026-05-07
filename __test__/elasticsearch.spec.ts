import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('elasticsearch', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer(
      'docker.elastic.co/elasticsearch/elasticsearch:8.13.0',
    )
      .withExposedPorts(9200)
      .withEnvironment({
        'discovery.type': 'single-node',
        'xpack.security.enabled': 'false',
        ES_JAVA_OPTS: '-Xms512m -Xmx512m',
      })
      .withWaitStrategy(Wait.forHttp('/_cluster/health', 9200))
      .withStartupTimeout(180_000)
      .start()
    databases.elasticsearch.host = container.getHost()
    databases.elasticsearch.port = container.getMappedPort(9200)
  }, 240_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('elasticsearch')
})
