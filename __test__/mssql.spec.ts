import { afterAll, beforeAll, describe } from 'vitest'
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('mssql', () => {
  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('mcr.microsoft.com/mssql/server:2022-latest')
      .withExposedPorts(1433)
      .withEnvironment({
        ACCEPT_EULA: 'Y',
        MSSQL_PID: 'Developer',
        MSSQL_SA_PASSWORD: 'Ueberdb!2025',
      })
      .withWaitStrategy(Wait.forLogMessage(/SQL Server is now ready for client connections/))
      .withStartupTimeout(180_000)
      .start()
    databases.mssql.host = container.getHost()
    databases.mssql.port = container.getMappedPort(1433)
  }, 240_000)

  afterAll(async () => {
    if (container) await container.stop()
  })

  test_db('mssql')
})
