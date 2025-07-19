
import { Postgres } from '../index.js'

import { expect, test, describe, afterAll, afterEach, beforeAll } from 'vitest'
import { GenericContainer, PortWithOptionalBinding, StartedTestContainer } from 'testcontainers'


const SKIP_TESTS = (process.env.CI_SKIP != null);

(SKIP_TESTS ? describe.skip : describe.sequential)('couch db tests', async () => {
  const portMappings: PortWithOptionalBinding[] = [{ container: 5432, host: 5432 }]

  let db: Postgres

  let container: StartedTestContainer

  const getCouchDB = ()=>{
    if (process.env['CI'] == null) {
      return new Postgres({
        host: 'localhost',
        database: 'ueberdb',
        port: 5432,
        user: 'ueberdb',
        password: 'ueberdb',
      })
    } else {
      return new Postgres({
        host: "postgres",
        database: 'ueberdb',
        port: 5432,
        user: 'ueberdb',
        password: 'ueberdb',
      })
    }
  }

  beforeAll(async () => {
    console.log('CI Environment:', process.env['CI'])
    if (process.env['CI'] == null) {
      container = await new GenericContainer("postgres:latest")
        .withExposedPorts(...portMappings)
        .withEnvironment({
          POSTGRES_USER: "ueberdb",
          POSTGRES_PASSWORD: "ueberdb",
          POSTGRES_DB: "ueberdb"
        }).withHealthCheck({
          test: ["CMD-SHELL", "pg_isready"],
          interval: 30,
          timeout: 60,
          retries: 5,
        })

        .start()

    }

    db = getCouchDB()
  })

  test('test get', async () => {
    db.set('key:test', 'value:test')
    let res = db.get('key:test')
    expect(res).toBe('value:test')
    db.remove('key:test')
    expect(db.get('key:test')).toBeNull()
  })

  test('test remove', () => {
    db.set('key:test', 'value:test')
    db.remove('key:test')
    expect(db.get('key:test')).toBeNull()
  })

  test('Key value set', async () => {
    db.set('key:test', 'value:test')
    let res = db.get('key:test')
    expect(res).toBe('value:test')
  })

  test('2 db open', async () => {
    db.set('test1', 'test2')
    const db2 = getCouchDB()
    db2.set('test2', 'test2')
  })

  test('Key value remove', async () => {
    db.set('key:test', 'value:test')
    db.remove('key:test')
    let res = db.get('key:test')
    expect(res).toBeNull()
  })

  test('Key value findKeys 2', async () => {
    db.set('key:test', 'value:test')
    db.set('key:test2', 'value:test2')
    db.set('key:123', 'value:123')
    let res = db.findKeys('key:test*')
    expect(res).toEqual(['key:test', 'key:test2'])
  })

  test('Key value findKeys', async () => {
    db.set('key:test', 'value:test')
    db.set('key:test2', 'value:test2')
    db.set('key:123', 'value:123')
    let res = db.findKeys('key:test2*')
    expect(res).toEqual(['key:test2'])
  })

  test('Key value findKeys rev', async () => {
    db.set('key:2:test', 'value:test')
    db.set('key:3:test2', 'value:test2')
    db.set('key:4:123', 'value:123')
    let res = db.findKeys('key:*:test')
    expect(res).toEqual(['key:2:test'])
  })

  test('Key value findKeys none', async () => {
    db.set('key:2:test', 'value:test')
    db.set('key:3:test2', 'value:test2')
    db.set('key:4:123', 'value:123')
    let res = db.findKeys('key:*5:test')
    expect(res).toEqual([])
  })

  test('Key value findKeys 45', async () => {
    db.set('key:2:test', 'value:test')
    db.set('key:3:test2', 'value:test2')
    db.set('key:4:123', 'value:123')
    db.set('key:45:test', 'value:123')
    let res = db.findKeys('key:*5:test')
    expect(res).toEqual(['key:45:test'])
  })

  test('findKeys with exclusion works', async () => {
    db.set('key:2:test', 'test')
    db.set('key:2:testa', 'true')
    db.set('key:2:testb', 'true')
    db.set('key:2:testb2', 'true')
    db.set('nonmatching_key:2:test', 'true')
    const keys = db.findKeys('key:2:test*', 'key:2:testb*')
    expect(keys.sort()).toStrictEqual(['key:2:test', 'key:2:testa'])
  })

  test('findKeys with no matches works', async () => {
    db.set('key:2:test', 'test')
    const keys = db.findKeys('123', 'key:2:testb*')
    expect(keys).toStrictEqual([])
  })

  test('find keys with no wildcards works', () => {
    db.set('key:2:test', '')
    db.set('key:2:testa', '')
    const keys = db.findKeys('key:2:testa')
    expect(keys).toStrictEqual(['key:2:testa'])
  })

  test('get without table', () => {
    db.get('234dsfsdfsdf')
  })

  afterEach(async () => {
    db.destroy()
  })

  afterAll(async () => {
    if (process.env.CI == null) {
      await container.stop()
    }
  })
})

