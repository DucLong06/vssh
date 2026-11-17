import { expect, test } from 'vitest'
import type { Peek } from './api'
import fixture from './fixtures/peek.json'
import { compact, parts } from './stats'

const peek = fixture as unknown as Peek

test('the figures are worded as the TUI status line words them', () => {
  // The brief's own example, and the core's test of `Stats::parts`.
  expect(compact(parts(peek.stats, 'MEM'))).toBe('CPU 23% · MEM 4.2/16G · / 61%')
  const small = parts({ cpu: 250, mem: [460_000, 512_000], disk: 85 }, 'RAM')
  expect(compact(small)).toBe('CPU 250% · RAM 0.4/0.5G · / 85%')
  expect(small.map((part) => part.level)).toEqual(['crit', 'crit', 'warn'])
  // A bar is never longer than its track.
  expect(small.map((part) => part.pct)).toEqual([100, 89, 85])
})

test('cpu warns over 50 and is critical over 80; memory over 70 and 85; disk over 80 and 90', () => {
  const cpu = (pct: number) => parts({ cpu: pct, mem: null, disk: null }, '')[0].level
  expect([0, 50, 51, 80, 81, 100].map(cpu)).toEqual(['ok', 'ok', 'warn', 'warn', 'crit', 'crit'])
  const mem = (pct: number) => parts({ cpu: null, mem: [pct, 100], disk: null }, '')[0].level
  expect([70, 71, 85, 86].map(mem)).toEqual(['ok', 'warn', 'warn', 'crit'])
  const disk = (pct: number) => parts({ cpu: null, mem: null, disk: pct }, '')[0].level
  expect([80, 81, 90, 91].map(disk)).toEqual(['ok', 'warn', 'warn', 'crit'])
})

test('what a host did not answer is left out, not made up', () => {
  expect(compact(parts({ cpu: 75, mem: null, disk: 61 }, 'MEM'))).toBe('CPU 75% · / 61%')
  expect(parts({ cpu: null, mem: null, disk: null }, 'MEM')).toEqual([])
  expect(compact(parts({ cpu: 0, mem: null, disk: 0 }, 'MEM'))).toBe('CPU 0% · / 0%')
})
