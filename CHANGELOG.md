# Changelog

## 0.1.0 (unreleased)

Initial release of `ueberdb-rs` — a Rust port of `ueberdb2` exposed via
napi-rs.

### Added

- Rust crate `ueberdb-rs` exposing a single `Database` napi class.
- Backends (all 14 from the TS package): memory, dirty, dirty_git, sqlite,
  rusty, postgres (single + pool), mysql/maria, mssql, mongodb, redis,
  couch, cassandra, elasticsearch, surrealdb.
- Wrapper layer in safe Rust:
  - `moka` LRU read-cache.
  - Coalescing write-buffer with periodic flush, `do_bulk` batching, and a
    snapshot/commit drain that's safe under concurrent backend updates.
  - Per-key tokio mutex map so different keys make progress in parallel.
  - `getSub`/`setSub` over `serde_json::Value` (passing `undefined`
    deletes the leaf, matching ueberDB).
  - `findKeys` glob translation; backends with native pattern matching
    short-circuit.
  - Atomic metrics counters via `db.metrics()`.
- Vitest conformance suite ported from ueberDB's `test/lib`, parameterized
  over `{readCache, writeBuffer}` × per-backend testcontainers spec.

### Changed

- Single `.node` artifact per platform; all driver crates always linked.
- Error messages preserved verbatim from the TS implementation so consumers
  that string-match errors keep working.
