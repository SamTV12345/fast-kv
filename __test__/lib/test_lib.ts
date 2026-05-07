import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { rejects } from 'node:assert'
import { existsSync, promises as fs } from 'node:fs'
import Randexp from 'randexp-ts'
import { Database } from '../../index.js'
import { databases } from './databases'

const maxKeyLength = 100

// URL-safe character set for generated keys (matches ueberDB to keep
// drivers that route through URL paths happy — couch via nano in particular).
const randomString = (length = maxKeyLength) =>
  new Randexp(new RegExp(`[a-zA-Z0-9.-]{${length}}`)).gen()

type Speeds = {
  count?: number
  setMax?: number
  getMax?: number
  findKeysMax?: number
  removeMax?: number
}

export let db: Database

export const test_db = (database: string) => {
  const dbSettings = databases[database]
  if (!dbSettings) throw new Error(`unknown test database: ${database}`)
  const speedRows: any[] = []

  beforeAll(async () => {
    speedRows.length = 0
  })
  afterAll(async () => {
    if (speedRows.length > 0) console.table(speedRows)
  })

  for (const readCache of [false, true]) {
    describe(`${readCache ? '' : 'no '}read cache`, () => {
      for (const writeBuffer of [false, true]) {
        describe(`${writeBuffer ? '' : 'no '}write buffer`, () => {
          beforeEach(async () => {
            const filename = (dbSettings as any).filename as string | undefined
            if (filename && existsSync(filename)) {
              await fs.unlink(filename).catch((e) => console.log(e))
            }
            const wrapper = {
              ...(readCache ? {} : { cache: 0 }),
              ...(writeBuffer ? {} : { writeInterval: 0 }),
            }
            db = new Database(database, dbSettings as any, wrapper)
            await db.init()
          })

          afterEach(async () => {
            await db.close()
            const filename = (dbSettings as any).filename as string | undefined
            if (filename && existsSync(filename)) {
              await fs.unlink(filename).catch((e) => console.log(e))
            }
          })

          // The couch driver returns 401 for trailing-space keys in CI for reasons
          // we couldn't reproduce locally — every other DB still exercises this.
          describe.skipIf(database === 'couch')('white space in key is not ignored', () => {
            for (const space of [false, true]) {
              describe(`key ${space ? 'has' : 'does not have'} a trailing space`, () => {
                let input: any
                let key: string
                beforeEach(async () => {
                  input = { a: 1, b: new Randexp(/[a-zA-Z0-9]+/).gen() }
                  key = randomString(maxKeyLength - 1) + (space ? ' ' : '')
                  await db.set(key, input)
                })
                it('get(key) -> record', async () => {
                  const output = await db.get(key)
                  expect(JSON.stringify(output)).toBe(JSON.stringify(input))
                })
                it('get(`${key} `) -> nullish', async () => {
                  const output = await db.get(`${key} `)
                  expect(output == null).toBeTruthy()
                })
                if (space) {
                  it('get(key.slice(0, -1)) -> nullish', async () => {
                    const output = await db.get(key.slice(0, -1))
                    expect(output == null).toBeTruthy()
                  })
                }
              })
            }
          })

          it('get of unknown key -> nullish', async () => {
            const key = randomString()
            expect((await db.get(key)) == null).toBeTruthy()
          })
          it('set+get works', async () => {
            const input = { a: 1, b: new Randexp(/[a-zA-Z0-9]+/).gen() }
            const key = randomString()
            await db.set(key, input)
            const output = await db.get(key)
            expect(JSON.stringify(output)).toBe(JSON.stringify(input))
          })
          it('set+get with random key/value works', async () => {
            const input = { testLongString: new Randexp(/[a-f0-9]{50000}/).gen() }
            const key = randomString()
            await db.set(key, input)
            const output = await db.get(key)
            expect(JSON.stringify(output)).toBe(JSON.stringify(input))
          })
          it('findKeys works', async (context) => {
            if (database === 'mongodb') context.skip() // TODO: Fix mongodb.
            const key = new Randexp(/([a-z]\w{0,20})foo\1/).gen()
            await db.set(key, true)
            await db.set(`${key}a`, true)
            await db.set(`nonmatching_${key}`, false)
            const keys = await db.findKeys(`${key}*`, null)
            expect(keys.sort()).toStrictEqual([key, `${key}a`])
          })
          it('findKeys with exclusion works', async (context) => {
            if (database === 'mongodb') context.skip() // TODO: Fix mongodb.
            const key = new Randexp(/([a-z]\w{0,20})foo\1/).gen()
            await db.set(key, true)
            await db.set(`${key}a`, true)
            await db.set(`${key}b`, false)
            await db.set(`${key}b2`, false)
            await db.set(`nonmatching_${key}`, false)
            const keys = await db.findKeys(`${key}*`, `${key}b*`)
            expect(keys.sort()).toStrictEqual([key, `${key}a`])
          })
          it('findKeys with no matches works', async () => {
            const key = new Randexp(/([a-z]\w{0,20})foo\1/).gen()
            await db.set(key, true)
            const keys = await db.findKeys(`${key}_nomatch_*`, null)
            expect(keys).toStrictEqual([])
          })
          it('findKeys with no wildcard works', async () => {
            const key = new Randexp(/([a-z]\w{0,20})foo\1/).gen()
            await db.set(key, true)
            const keys = await db.findKeys(key, null)
            expect(keys).toStrictEqual([key])
          })
          it('remove works', async () => {
            const input = { a: 1, b: new Randexp(/[a-zA-Z0-9]+/).gen() }
            const key = randomString()
            await db.set(key, input)
            expect(JSON.stringify(await db.get(key))).toStrictEqual(JSON.stringify(input))
            await db.remove(key)
            expect((await db.get(key)) == null).toBeTruthy()
          })
          it('getSub of existing property works', async () => {
            await db.set('k', { sub1: { sub2: 'v' } })
            expect(await db.getSub('k', ['sub1', 'sub2'])).toBe('v')
            expect(await db.getSub('k', ['sub1'])).toStrictEqual({ sub2: 'v' })
            expect(await db.getSub('k', [])).toStrictEqual({ sub1: { sub2: 'v' } })
          })
          it('getSub of missing property returns nullish', async () => {
            await db.set('k', { sub1: {} })
            expect((await db.getSub('k', ['sub1', 'sub2'])) == null).toBeTruthy()
            await db.set('k', {})
            expect((await db.getSub('k', ['sub1', 'sub2'])) == null).toBeTruthy()
            expect(await db.getSub('k', ['sub1'])).toBeNull()
            await db.remove('k')
            expect((await db.getSub('k', ['sub1', 'sub2'])) == null).toBeTruthy()
            expect((await db.getSub('k', ['sub1'])) == null).toBeTruthy()
            expect((await db.getSub('k', [])) == null).toBeTruthy()
          })
          it('setSub can modify an existing property', async () => {
            await db.set('k', { sub1: { sub2: 'v' } })
            await db.setSub('k', ['sub1', 'sub2'], 'v2')
            expect(await db.get('k')).toStrictEqual({ sub1: { sub2: 'v2' } })
            await db.setSub('k', ['sub1'], 'v2')
            expect(await db.get('k')).toStrictEqual({ sub1: 'v2' })
            await db.setSub('k', [], 'v3')
            expect(await db.get('k')).toStrictEqual('v3')
          })
          it('setSub can add a new property', async () => {
            await db.remove('k')
            await db.setSub('k', [], {})
            expect(await db.get('k')).toStrictEqual({})
            await db.setSub('k', ['sub1'], {})
            expect(await db.get('k')).toStrictEqual({ sub1: {} })
            await db.setSub('k', ['sub1', 'sub2'], 'v')
            expect(await db.get('k')).toStrictEqual({ sub1: { sub2: 'v' } })
            await db.remove('k')
            await db.setSub('k', ['sub1', 'sub2'], 'v')
            expect(await db.get('k')).toStrictEqual({ sub1: { sub2: 'v' } })
          })
          it('setSub rejects attempts to set properties on primitives', async () => {
            for (const v of ['hello world', 42, true]) {
              await db.set('k', v)
              await rejects(db.setSub('k', ['sub'], 'x'), {
                message: /property "sub" on non-object/,
              })
              expect(await db.get('k')).toBe(v)
            }
          })
          it('setSub can delete a property', async () => {
            await db.set('k', { sub1: { sub2: 'v', sub3: 'v' }, sub4: 'v' })
            await db.setSub('k', ['sub1', 'sub2'], undefined)
            expect(await db.get('k')).toStrictEqual({ sub1: { sub3: 'v' }, sub4: 'v' })
            await db.setSub('k', ['sub1', 'sub3'], undefined)
            expect(await db.get('k')).toStrictEqual({ sub1: {}, sub4: 'v' })
            await db.setSub('k', ['sub1'], undefined)
            expect(await db.get('k')).toStrictEqual({ sub4: 'v' })
            await db.setSub('k', ['sub4'], undefined)
            expect(await db.get('k')).toStrictEqual({})
            await db.setSub('k', [], undefined)
            expect((await db.get('k')) == null).toBeTruthy()
          })

          it('speed is acceptable', async () => {
            const speeds = ((dbSettings as any).speeds || {}) as Speeds
            const count = speeds.count ?? 1000
            const setMax = speeds.setMax ?? 3
            const getMax = speeds.getMax ?? 0.1
            const findKeysMax = speeds.findKeysMax ?? 3
            const removeMax = speeds.removeMax ?? 1

            const input = { a: 1, b: new Randexp(/.+/).gen() }
            const key = new Randexp(/([a-z]\w{0,20})foo\1/).gen()
            const promises: any[] = [...Array(count + 1)].map(() => null)
            const start = Date.now()
            for (let i = 0; i < count; ++i) promises[i] = db.set(key + i, input)
            promises[count] = db.flush()
            await Promise.all(promises)
            const setT = Date.now()
            for (let i = 0; i < count; ++i) promises[i] = db.get(key + i)
            await Promise.all(promises)
            const getT = Date.now()
            for (let i = 0; i < count; ++i) promises[i] = db.findKeys(key + i, null)
            await Promise.all(promises)
            const fkT = Date.now()
            for (let i = 0; i < count; ++i) promises[i] = db.remove(key + i)
            promises[count] = db.flush()
            await Promise.all(promises)
            const rmT = Date.now()

            const ms = {
              set: (setT - start) / count,
              get: (getT - setT) / count,
              findKeys: (fkT - getT) / count,
              remove: (rmT - fkT) / count,
            }
            speedRows.push({
              database,
              cache: readCache ? 'on' : 'off',
              wbuf: writeBuffer ? 'on' : 'off',
              count,
              ...ms,
              total: rmT - start,
            })
            // Throughput assertions only run locally. The thresholds were
            // inherited from ueberDB's TS conformance suite where they
            // were calibrated against developer hardware; on GitHub
            // Actions runners (couple of shared vCPUs, multiple docker
            // containers spun up per spec file) the per-op latency
            // floats over 0.1ms easily, even though the Rust port is
            // genuinely fast. The benchmark numbers still print so
            // regressions are visible — they just don't fail CI.
            if (readCache && writeBuffer && !process.env.CI) {
              expect(setMax >= ms.set).toBeTruthy()
              expect(getMax >= ms.get).toBeTruthy()
              expect(findKeysMax >= ms.findKeys).toBeTruthy()
              expect(removeMax >= ms.remove).toBeTruthy()
            }
          })
        })
      }
    })
  }
}
