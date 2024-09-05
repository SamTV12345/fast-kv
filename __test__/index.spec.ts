
import { KeyValueDB } from '../index.js'

import {expect, test} from 'vitest'


test('test get', () => {
  const db = new KeyValueDB('test')
  db.set('key:test', 'value:test')
  let res = db.get('key:test');
  expect(res).toBe('value:test')
  db.remove("key:test")
  expect(db.get('key:test')).toBeNull()
  db.destroy()
})

test('test remove', () => {
  const db = new KeyValueDB('test1')
  db.set('key:test', 'value:test')
  db.remove('key:test')
  expect(db.get('key:test')).toBeNull()
  db.destroy()
})

test('Key value set', ()=>{
  const db = new KeyValueDB('test2')
  db.set('key:test', 'value:test')
  let res = db.get('key:test')
  expect(res).toBe('value:test')
  db.destroy()
})


test('2 db open', ()=>{
  const db = new KeyValueDB('test')
  db.set('test1', "test2")
  db.close()
  const db2 = new KeyValueDB('test')
  db2.set('test2', 'test2')
  db2.close()
})

test('Key value remove', ()=>{
  const db = new KeyValueDB('test3')
  db.set('key:test', 'value:test')
  db.remove('key:test')
  let res = db.get('key:test')
  expect(res).toBeNull()
  db.destroy()
})


test('Key value findKeys 2', ()=>{
  const db = new KeyValueDB('test4')
  db.set('key:test', 'value:test')
  db.set('key:test2', 'value:test2')
  db.set('key:123', "value:123")
  let res = db.findKeys('key:test*')
  expect(res).toEqual(['key:test', 'key:test2'])
  db.destroy()
})

test('Key value findKeys', ()=>{
  const db = new KeyValueDB('test5')
  db.set('key:test', 'value:test')
  db.set('key:test2', 'value:test2')
  db.set('key:123', "value:123")
  let res = db.findKeys('key:test2*')
  expect(res).toEqual(['key:test2'])
  db.destroy()
})

test('Key value findKeys rev', ()=>{
  const db = new KeyValueDB('test6')
  db.set('key:2:test', 'value:test')
  db.set('key:3:test2', 'value:test2')
  db.set('key:4:123', "value:123")
  let res = db.findKeys('key:*:test')
  expect(res).toEqual(['key:2:test'])
  db.destroy()
})

test('Key value findKeys none', ()=>{
  const db = new KeyValueDB('test7')
  db.set('key:2:test', 'value:test')
  db.set('key:3:test2', 'value:test2')
  db.set('key:4:123', "value:123")
  let res = db.findKeys('key:*5:test')
  expect(res).toEqual([])
  db.destroy()
})

test('Key value findKeys 45', ()=>{
  const db = new KeyValueDB('test8')
  db.set('key:2:test', 'value:test')
  db.set('key:3:test2', 'value:test2')
  db.set('key:4:123', "value:123")
  db.set('key:45:test', "value:123")
  let res = db.findKeys('key:*5:test')
  expect(res).toEqual(["key:45:test"])
  db.destroy()
})


test('findKeys with exclusion works', ()=>{
  const db = new KeyValueDB('test9')
  db.set('key:2:test','test')
  db.set('key:2:testa', 'true')
  db.set('key:2:testb', 'true')
  db.set('key:2:testb2', 'true')
  db.set('nonmatching_key:2:test', 'true')
  const keys = db.findKeys('key:2:test*', "key:2:testb*")
  console.log(keys)
  expect(keys.sort()).toStrictEqual(['key:2:test', 'key:2:testa'])
  db.destroy()
})


test('findKeys with no matches works', async ()=>{
  const db = new KeyValueDB('test10')
  db.set('key:2:test','test')
  const keys = db.findKeys('123', "key:2:testb*")
  expect(keys).toStrictEqual([])
  db.destroy()
})


test('find keys with no wildcards works', async ()=>{
  const db = new KeyValueDB('test11')
  db.set('key:2:test','')
  db.set('key:2:testa', '')
  const keys = db.findKeys('key:2:testa')
  expect(keys).toStrictEqual(['key:2:testa'])
  db.destroy()
})

test('get without table', async ()=>{
  const db = new KeyValueDB('test12')
  db.get('234dsfsdfsdf')
  db.destroy()
})