import { beforeAll, describe } from 'vitest'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { databases } from './lib/databases'
import { test_db } from './lib/test_lib'

describe('dirty_git', () => {
  beforeAll(() => {
    // Each run gets a fresh per-test directory so libgit2 starts from
    // an empty repository state. test_lib's afterEach deletes the dirty
    // file but the repo dir is reused across the four read-cache ×
    // write-buffer combos in this describe.
    const dir = mkdtempSync(join(tmpdir(), 'ueberdb-dirtygit-'))
    databases.dirty_git.filename = join(dir, 'dirty.db')
  })

  test_db('dirty_git')
})
