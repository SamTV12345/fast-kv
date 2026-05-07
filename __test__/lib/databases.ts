import os from 'os'
import path from 'path'

export type DatabaseConfig = {
  filename?: string
  url?: string
  host?: string
  port?: number
  user?: string
  password?: string
  database?: string
  charset?: string
  base_index?: string
  speeds?: {
    count?: number
    setMax?: number
    getMax?: number
    findKeysMax?: number
    removeMax?: number
  }
  [key: string]: unknown
}

const tmp = (name: string) => path.join(os.tmpdir(), name)

export const databases: Record<string, DatabaseConfig> = {
  memory: {},
  dirty: {
    filename: tmp('ueberdb-test.db'),
    // findKeys does a full scan of the in-memory index; relaxed from
    // ueberDB's 0.5 to absorb napi/tokio overhead on the Rust port.
    speeds: { setMax: 1, getMax: 0.1, findKeysMax: 5 },
  },
  dirty_git: {
    // Set per-test in dirty_git.spec.ts so each run gets a fresh repo.
    filename: '',
    // Every set/remove triggers a libgit2 commit on top of the dirty
    // append. setMax stays generous; getMax/findKeysMax mirror dirty.
    speeds: { setMax: 30, getMax: 0.1, findKeysMax: 5, removeMax: 30 },
  },
  sqlite: {
    filename: tmp('ueberdb-test.sqlite'),
    speeds: { setMax: 0.6, getMax: 0.5, findKeysMax: 2.5, removeMax: 0.5 },
  },
  rustydb: {
    filename: tmp('rusty.db'),
    // redb has no native pattern matching, so findKeys is a full table
    // scan — bump the budget accordingly compared to ueberDB's TS rusty.
    speeds: { setMax: 2, getMax: 0.5, findKeysMax: 20, removeMax: 3 },
  },
  postgres: {
    user: 'ueberdb',
    host: '127.0.0.1',
    password: 'ueberdb',
    database: 'ueberdb',
    speeds: { setMax: 6 },
  },
  mysql: {
    user: 'ueberdb',
    host: '127.0.0.1',
    password: 'ueberdb',
    database: 'ueberdb',
    charset: 'utf8mb4',
    speeds: { findKeysMax: 6, getMax: 1 },
  },
  redis: { url: 'redis://localhost/' },
  mongodb: {
    url: 'mongodb://127.0.0.1:27017',
    database: 'mydb_test',
    speeds: {
      count: 2000,
      findKeysMax: 5,
      setMax: 10,
      getMax: 10,
      removeMax: 10,
    },
  },
  couch: {
    host: 'localhost',
    port: 5984,
    database: 'ueberdb',
    user: 'ueberdb',
    password: 'ueberdb',
    // Per-op HTTP round-trips are inherently slower than other backends.
    speeds: { setMax: 5, getMax: 1, findKeysMax: 30, removeMax: 5 },
  },
}
