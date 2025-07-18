
import { Couch } from '../index.js'

import { expect, test, describe, afterAll, afterEach, beforeAll, beforeEach } from 'vitest'
import { GenericContainer, PortWithOptionalBinding, StartedTestContainer } from 'testcontainers'


describe.sequential('couch db tests', async () => {
  const portMappings: PortWithOptionalBinding[] = [{ container: 5984, host: 5984 }]

  let db: Couch

  let container: StartedTestContainer

  beforeAll(async () => {
    container = await new GenericContainer('couchdb:latest')
      .withExposedPorts(...portMappings)
      .withEnvironment({
        COUCHDB_USER: 'ueberdb',
        COUCHDB_PASSWORD: 'ueberdb',
      })
      .withHealthCheck({
        test: ['CMD-SHELL', 'curl -f http://localhost:5984/_up || exit 1'],
        interval: 10000,
        timeout: 5000,
        retries: 5,
      })
      .start()

      db = new Couch({
      host: 'localhost',
      database: 'test',
      port: 5984,
      user: 'ueberdb',
      password: 'ueberdb',
    })
  })

  beforeEach(async ()=>{
    await db.init()
  })

  test('test get', async () => {
    await db.set('key:test', 'value:test')
    let res = await db.get('key:test')
    expect(res).toBe('value:test')
    await db.remove('key:test')
    expect(await db.get('key:test')).toBeNull()
  })

  test('test remove', async () => {
    await db.set('key:test', 'value:test')
    await db.remove('key:test')
    expect(await db.get('key:test')).toBeNull()
  })

  test('Key value set', async () => {
    await db.set('key:test', 'value:test')
    let res = await db.get('key:test')
    expect(res).toBe('value:test')
  })

  test('2 db open', async () => {
    await db.set('test1', 'test2')
    const db2 = new Couch({
      host: 'localhost',
      database: 'test',
      port: 5984,
      user: 'ueberdb',
      password: 'ueberdb',
    })
    await db2.init()
    await db2.set('test2', 'test2')
  })

  test('Key value remove', async () => {
    await db.set('key:test', 'value:test')
    await db.remove('key:test')
    let res = await db.get('key:test')
    expect(res).toBeNull()
  })

  test('Key value findKeys 2', async () => {
    await db.set('key:test', 'value:test')
    await db.set('key:test2', 'value:test2')
    await db.set('key:123', 'value:123')
    let res = await db.findKeys('key:test*')
    expect(res).toEqual(['key:test', 'key:test2'])
  })

  test('Key value findKeys', async () => {
    await db.set('key:test', 'value:test')
    await db.set('key:test2', 'value:test2')
    await db.set('key:123', 'value:123')
    let res = await db.findKeys('key:test2*')
    expect(res).toEqual(['key:test2'])
  })

  test('Key value findKeys rev', async () => {
    await db.set('key:2:test', 'value:test')
    await db.set('key:3:test2', 'value:test2')
    await db.set('key:4:123', 'value:123')
    let res = await db.findKeys('key:*:test')
    expect(res).toEqual(['key:2:test'])
  })

  test('Key value findKeys none', async () => {
    await db.set('key:2:test', 'value:test')
    await db.set('key:3:test2', 'value:test2')
    await db.set('key:4:123', 'value:123')
    let res = await db.findKeys('key:*5:test')
    expect(res).toEqual([])
  })

  test('Key value findKeys 45', async () => {
    await db.set('key:2:test', 'value:test')
    await db.set('key:3:test2', 'value:test2')
    await db.set('key:4:123', 'value:123')
    await db.set('key:45:test', 'value:123')
    let res = await db.findKeys('key:*5:test')
    expect(res).toEqual(['key:45:test'])
  })

  test('findKeys with exclusion works', async () => {
    await db.set('key:2:test', 'test')
    await db.set('key:2:testa', 'true')
    await db.set('key:2:testb', 'true')
    await db.set('key:2:testb2', 'true')
    await db.set('nonmatching_key:2:test', 'true')
    const keys = await db.findKeys('key:2:test*', 'key:2:testb*')
    expect(keys.sort()).toStrictEqual(['key:2:test', 'key:2:testa'])
  })

  test('findKeys with no matches works', async () => {
    await db.set('key:2:test', 'test')
    const keys = await db.findKeys('123', 'key:2:testb*')
    expect(keys).toStrictEqual([])
  })

  test('find keys with no wildcards works', async () => {
    await db.set('key:2:test', '')
    await db.set('key:2:testa', '')
    const keys = await db.findKeys('key:2:testa')
    expect(keys).toStrictEqual(['key:2:testa'])
  })

  test('get without table', async () => {
    await db.get('234dsfsdfsdf')
  })

  afterEach(async () => {
    await db.destroy()
  })

  afterAll(async () => {
    await container.stop()
  })
})

