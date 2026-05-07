# ueberdb2 (Rust port)

Drop-in replacement for [`ueberdb2`](https://github.com/ether/ueberDB) — the
multi-backend key/value abstraction Etherpad uses — re-implemented in safe
Rust behind a [napi-rs](https://napi.rs/) binding. Same `Database` JS class,
same `Settings` shape, same Promise-returning method surface.

## Features

A single `Database` class wraps a `Backend` trait plus a wrapper layer:

- **LRU read-cache** (moka) with per-Settings capacity
- **Coalescing write-buffer** with periodic flush, `do_bulk` batching, and
  a snapshot/commit drain that's safe under concurrent backend updates
- **Per-key locks** so different keys make progress in parallel while writes
  to the same key stay serialized
- **`getSub` / `setSub`** for nested JSON, with TS-parity semantics
  (passing `undefined` deletes the leaf)
- **`findKeys`** glob translation; backends with native pattern matching
  short-circuit
- **Atomic counters** for reads/writes/cache hits/flushes/bulks via
  `db.metrics()`

## Backends

All 14 ueberDB backends are ported. Each row is exercised by the same
vitest conformance suite (memory and dirty-style backends from ueberDB
plus per-backend testcontainers specs).

| `type` arg          | Crate / driver        | Native pattern matching |
| ------------------- | --------------------- | ----------------------- |
| `memory`            | `HashMap` + `RwLock`  | wrapper-side            |
| `dirty`             | append-log file       | wrapper-side            |
| `dirty_git`         | `dirty` + `git2`      | wrapper-side            |
| `sqlite`            | `sqlx` (sqlite)       | `LIKE`                  |
| `rusty` / `rustydb` | `redb`                | wrapper-side            |
| `postgres`          | `tokio-postgres`      | `LIKE`                  |
| `postgrespool`      | `tokio-postgres`+`bb8`| `LIKE`                  |
| `mysql` / `mariadb` | `sqlx` (mysql)        | `LIKE`                  |
| `mssql`             | `tiberius`            | `LIKE`                  |
| `mongodb`           | `mongodb`             | regex                   |
| `redis`             | `redis`               | `SCAN MATCH`            |
| `couch`             | `reqwest` (HTTP API)  | wrapper-side            |
| `cassandra`         | `scylla`              | wrapper-side            |
| `elasticsearch`     | `reqwest` (HTTP API)  | wildcard query          |
| `surrealdb`         | `reqwest` (HTTP API)  | wrapper-side            |

## JS API

```ts
import { Database } from 'ueberdb2'

const db = new Database('postgres', {
  host: 'localhost',
  user: 'ueberdb',
  password: 'ueberdb',
  database: 'ueberdb',
}, {
  cache: 1000,        // LRU capacity (0 disables)
  writeInterval: 100, // ms between background flushes (0 disables)
  bulkLimit: 100,     // max ops per bulk flush
})

await db.init()
await db.set('key', { a: 1 })
const value = await db.get('key')
const keys = await db.findKeys('key:*', null)
await db.setSub('key', ['a'], 2)
await db.flush()
await db.close()
```

## Testing

The conformance suite under `__test__/` is a port of ueberDB's `test/lib`,
parameterized over `{readCache, writeBuffer}` × per-backend container, and
runs ~80 cases per backend. The networked specs spin up the right Docker
image via [`testcontainers`](https://testcontainers.com/).

```bash
pnpm install
pnpm build:debug
pnpm test                          # all backends
pnpm test __test__/sqlite.spec.ts  # one backend
cargo test --lib                   # Rust unit tests for the wrapper layer
```

## Status

This is the Rust port branch. The npm name `ueberdb2` is currently still
served by the TypeScript package; this branch is published under the
`next` dist-tag until the conformance suite is green on every platform
in the CI matrix.

## License

Apache-2.0
