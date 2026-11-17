import { expect, test } from 'vitest'
import { fuzzy } from './fuzzy'

test('a query matches when its letters appear in order', () => {
  expect(fuzzy('web', 'web-01')).toBe(0)
  expect(fuzzy('w1', 'web-01')).not.toBeNull()
  expect(fuzzy('qc', 'Quick connect')).not.toBeNull()
  expect(fuzzy('1w', 'web-01')).toBeNull()
  expect(fuzzy('xyz', 'web-01')).toBeNull()
})

test('case, spaces and Vietnamese accents do not matter', () => {
  expect(fuzzy('WEB', 'web-01')).toBe(0)
  expect(fuzzy('ket noi', 'Kết nối nhanh')).toBe(0)
  expect(fuzzy('may chu', 'Máy chủ')).toBe(0)
  expect(fuzzy('đóng', 'dong')).toBe(0)
})

test('an empty query matches everything, and a tighter match scores lower', () => {
  expect(fuzzy('', 'anything')).toBe(0)
  expect(fuzzy('  ', 'anything')).toBe(0)
  expect(fuzzy('web', 'web-01')!).toBeLessThan(fuzzy('web', 'w-e-b')!)
  expect(fuzzy('db', 'db-primary')!).toBeLessThan(fuzzy('db', 'old-db')!)
})
