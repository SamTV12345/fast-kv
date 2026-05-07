import { describe } from 'vitest'
import { test_db } from './lib/test_lib'

describe('memory', () => {
  test_db('memory')
})
