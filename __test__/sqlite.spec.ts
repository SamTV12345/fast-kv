
import {  SQLite } from '../index.js'

import {expect, test} from 'vitest'


test('test get', () => {
  const db = new SQLite(':memory:')
  db.set('key:test', 'value:test')
  let res = db.get('key:test');
  expect(res).toBe('value:test')
  db.remove("key:test")
  expect(db.get('key:test')).toBeNull()
})

test('test remove', () => {
  const db = new SQLite(':memory:')
  db.set('key:test', 'value:test')
  db.remove('key:test')
  expect(db.get('key:test')).toBeNull()
})

test('Key value set', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:test', 'value:test')
  let res = db.get('key:test')
  expect(res).toBe('value:test')
})


test('2 db open', ()=>{
  const db = new SQLite('./test.sqlite')
  db.set('test1', "test2")
  db.close()
  const db2 = new SQLite('./test.sqlite')
  db2.set('test2', 'test2')
  db2.close()
})

test('Key value remove', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:test', 'value:test')
  db.remove('key:test')
  let res = db.get('key:test')
  expect(res).toBeNull()
})


test('Key value findKeys 2', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:test', 'value:test')
  db.set('key:test2', 'value:test2')
  db.set('key:123', "value:123")
  let res = db.findKeys('key:test*')
  expect(res).toEqual(['key:test', 'key:test2'])
})

test('Key value findKeys', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:test', 'value:test')
  db.set('key:test2', 'value:test2')
  db.set('key:123', "value:123")
  let res = db.findKeys('key:test2*')
  expect(res).toEqual(['key:test2'])
})

test('Key value findKeys rev', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:2:test', 'value:test')
  db.set('key:3:test2', 'value:test2')
  db.set('key:4:123', "value:123")
  let res = db.findKeys('key:*:test')
  expect(res).toEqual(['key:2:test'])
})

test('Key value findKeys none', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:2:test', 'value:test')
  db.set('key:3:test2', 'value:test2')
  db.set('key:4:123', "value:123")
  let res = db.findKeys('key:*5:test')
  expect(res).toEqual([])
})

test('Key value findKeys 45', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:2:test', 'value:test')
  db.set('key:3:test2', 'value:test2')
  db.set('key:4:123', "value:123")
  db.set('key:45:test', "value:123")
  let res = db.findKeys('key:*5:test')
  expect(res).toEqual(["key:45:test"])
})


test('findKeys with exclusion works', ()=>{
  const db = new SQLite(':memory:')
  db.set('key:2:test','test')
  db.set('key:2:testa', 'true')
  db.set('key:2:testb', 'true')
  db.set('key:2:testb2', 'true')
  db.set('nonmatching_key:2:test', 'true')
  const keys = db.findKeys('key:2:test*', "key:2:testb*")
  expect(keys.sort()).toStrictEqual(['key:2:test', 'key:2:testa'])
})


test('findKeys with no matches works', async ()=>{
  const db = new SQLite(':memory:')
  db.set('key:2:test','test')
  const keys = db.findKeys('123', "key:2:testb*")
  expect(keys).toStrictEqual([])
})


test('find keys with no wildcards works', async ()=>{
  const db = new SQLite(':memory:')
  db.set('key:2:test','')
  db.set('key:2:testa', '')
  const keys = db.findKeys('key:2:testa')
  expect(keys).toStrictEqual(['key:2:testa'])
})

test('get without table', async ()=>{
  const db = new SQLite(':memory:')
  db.get('234dsfsdfsdf')
})