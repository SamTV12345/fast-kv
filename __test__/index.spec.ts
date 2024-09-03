
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
    const db = new KeyValueDB('test')
    db.set('key:test', 'value:test')
    db.remove('key:test')
    expect(db.get('key:test')).toBeNull()
    db.destroy()
})

test('Key value set', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:test', 'value:test')
    let res = db.get('key:test')
    expect(res).toBe('value:test')
    db.destroy()
})

test('Key value remove', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:test', 'value:test')
    db.remove('key:test')
    let res = db.get('key:test')
    expect(res).toBeNull()
    db.destroy()
})


test('Key value findKeys 2', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:test', 'value:test')
    db.set('key:test2', 'value:test2')
    db.set('key:123', "value:123")
    let res = db.findKeys('key:test*')
    expect(res).toEqual(['key:test', 'key:test2'])
    db.destroy()
})

test('Key value findKeys', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:test', 'value:test')
    db.set('key:test2', 'value:test2')
    db.set('key:123', "value:123")
    let res = db.findKeys('key:test2*')
    expect(res).toEqual(['key:test2'])
    db.destroy()
})

test('Key value findKeys rev', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:2:test', 'value:test')
    db.set('key:3:test2', 'value:test2')
    db.set('key:4:123', "value:123")
    let res = db.findKeys('key:*:test')
    expect(res).toEqual(['key:2:test'])
    db.destroy()
})

test('Key value findKeys none', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:2:test', 'value:test')
    db.set('key:3:test2', 'value:test2')
    db.set('key:4:123', "value:123")
    let res = db.findKeys('key:*5:test')
    expect(res).toEqual([])
    db.destroy()
})

test('Key value findKeys 45', ()=>{
    const db = new KeyValueDB('test')
    db.set('key:2:test', 'value:test')
    db.set('key:3:test2', 'value:test2')
    db.set('key:4:123', "value:123")
    db.set('key:45:test', "value:123")
    let res = db.findKeys('key:*5:test')
    expect(res).toEqual(["key:45:test"])
    db.destroy()
})