import { expect, test } from 'vitest'
import { crumbs, eta, human, join, parent, when } from './paths'

test('paths, as the TUI joins and climbs them', () => {
  expect(join('/', 'a')).toBe('/a')
  expect(join('/home/u', 'a')).toBe('/home/u/a')
  expect(parent('/home/u')).toBe('/home')
  expect(parent('/home')).toBe('/')
  expect(parent('/')).toBe('/')
})

test('a path in crumbs: every directory on the way, each with where it is', () => {
  expect(crumbs('/var/log')).toEqual([
    { name: '/', path: '/' },
    { name: 'var', path: '/var' },
    { name: 'log', path: '/var/log' },
  ])
  expect(crumbs('/')).toEqual([{ name: '/', path: '/' }])
  // Not known yet.
  expect(crumbs('')).toEqual([])
})

test('sizes, in the TUI\'s units', () => {
  expect(human(0)).toBe('0 B')
  expect(human(1023)).toBe('1023 B')
  expect(human(1536)).toBe('1.5 K')
  expect(human(5 * 1024 * 1024)).toBe('5.0 M')
  expect(human(3 * 1024 ** 4)).toBe('3.0 T')
})

test('when a file changed, in local time, as ls has it; a time nobody knows is a dash', () => {
  const year = new Date().getFullYear()
  expect(when(new Date(year, 0, 6, 9, 5).getTime() / 1000)).toBe('01-06 09:05')
  // Before this year the day is enough.
  expect(when(new Date(2019, 2, 2, 9, 5).getTime() / 1000)).toBe('2019-03-02')
  expect(when(0)).toBe('—')
})

test('how long a transfer still takes', () => {
  expect(eta(42)).toBe('42s')
  expect(eta(185)).toBe('3m05s')
  expect(eta(3720)).toBe('1h02m')
  // Nothing has moved yet: nobody knows.
  expect(eta(Infinity)).toBe('—')
})
