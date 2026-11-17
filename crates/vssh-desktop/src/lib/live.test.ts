import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { afterEach, expect, test, vi } from 'vitest'
import type { ConnError, Peek } from './api'
import errors from './fixtures/errors.json'
import fixture from './fixtures/peek.json'
import shown from './fixtures/shown.json'
import { ask, forget, live, ping, poll, relabel, sick, trail, up } from './live.svelte'

const peek = fixture as unknown as Peek
const refused = errors[2]
type Args = { alias: string; passive: boolean; lang: 'en' | 'vi' }

/** A shell whose every host answers with `answer()`. */
function shell(answer: (args: Args) => unknown) {
  const asked: string[] = []
  mockIPC((cmd, args) => {
    const { alias, passive, lang } = args as Args
    if (cmd === 'resolve_conn_error') return shown[lang][2]
    asked.push(`${cmd} ${alias}${passive ? ' passive' : ''}`)
    return cmd === 'ping_host' ? 12 : answer(args as Args)
  })
  return asked
}

afterEach(() => {
  clearMocks()
  forget()
})

test('an answer is kept with the time it came', async () => {
  shell(() => peek)
  await ask('web-01', true)
  expect(live['web-01'].peek).toEqual(peek)
  expect(live['web-01'].at).toBeGreaterThan(Date.now() - 1000)
  expect(sick(live['web-01'])).toBe(false)
})

test('a failure is kept as it is and in words, and the host is sick', async () => {
  shell(() => {
    throw refused
  })
  await ask('web-01', true)
  expect(live['web-01'].error).toEqual(refused)
  expect(live['web-01'].shown).toEqual(shown.en[2])
  expect(live['web-01'].peek).toBeUndefined()
  expect(sick(live['web-01'])).toBe(true)

  // The words follow the language; the error itself stays.
  await relabel('vi')
  expect(live['web-01'].shown?.label).toBe('BỊ TỪ CHỐI')
  expect(live['web-01'].error).toEqual(refused)
})

test('a host the dashboard may not ask on its own is "not checked", and keeps what a person asked for', async () => {
  shell(({ passive }) => (passive ? null : peek))
  await ask('hidden', true)
  expect(live.hidden.peek).toBeNull()
  expect(sick(live.hidden)).toBe(false)
  await ask('hidden', false)
  expect(live.hidden.peek).toEqual(peek)
  // The next round of the poll does not wipe it.
  await ask('hidden', true)
  expect(live.hidden.peek).toEqual(peek)
})

test('a host is sick when a figure is critical or it does not answer a ping', async () => {
  expect(sick(undefined)).toBe(false)
  expect(sick({ peek: { ...peek, stats: { cpu: 81, mem: null, disk: null } } })).toBe(true)
  expect(sick({ peek: { ...peek, stats: { cpu: 80, mem: [80, 100], disk: 90 } } })).toBe(false)
  expect(sick({ ms: null })).toBe(true)
  expect(sick({ ms: 40, peek: null })).toBe(false)
})

test('a ping is the time to the host; one that cannot be pinged has none', async () => {
  shell(() => peek)
  await ping('web-01')
  expect(live['web-01'].ms).toBe(12)
  mockIPC(() => {
    throw { class: 'other', fail: null, chain: ['"hidden" is not reached directly'] }
  })
  await ping('hidden')
  expect(live.hidden.ms).toBeUndefined()
})

test('a round asks every host passively, pings when told to, and never asks one twice at a time', async () => {
  let answer = (_: unknown) => {}
  const asked = shell(() => new Promise((resolve) => (answer = resolve)))
  poll(['web-01', 'web-02'], true)
  poll(['web-01', 'web-02'], false)
  expect(asked.sort()).toEqual([
    'get_host_stats web-01 passive',
    'get_host_stats web-02 passive',
    'ping_host web-01',
    'ping_host web-02',
  ])
  answer(peek)
  await vi.waitFor(() => expect(live['web-02'].asking).toBe(false))
})

test('a hidden window asks nobody', () => {
  const asked = shell(() => peek)
  const hidden = vi.spyOn(document, 'hidden', 'get').mockReturnValue(true)
  poll(['web-01'], true)
  expect(asked).toEqual([])
  hidden.mockRestore()
})

// --- D9: the minute behind a figure --------------------------------------------

test('every answer adds a round to the host\'s trail: twelve at most, the oldest goes first', async () => {
  let round = 0
  shell(() => ({ ...peek, stats: { cpu: round++, mem: [round, 100], disk: null } }))
  for (let asked = 0; asked < 14; asked++) await ask('web-01', true)
  expect(trail['web-01'].cpu).toEqual([2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13])
  expect(trail['web-01'].mem).toEqual([3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14])
})

test('a figure the host did not give leaves no point, and a load past 100% is 100', async () => {
  shell(() => ({ ...peek, stats: { cpu: 250, mem: null, disk: 61 } }))
  await ask('web-01', true)
  expect(trail['web-01']).toEqual({ cpu: [100], mem: [] })
})

test('a host that fails loses its trail; one the dashboard may not ask has none', async () => {
  let answer: () => unknown = () => peek
  shell(({ passive }) => (passive ? answer() : null))
  await ask('web-01', true)
  expect(trail['web-01'].cpu).toHaveLength(1)
  answer = () => {
    throw refused
  }
  await ask('web-01', true)
  expect(trail['web-01']).toBeUndefined()
  answer = () => null
  await ask('hidden', true)
  expect(trail.hidden).toBeUndefined()
})

test('a host is up when its figures came, or a ping got through and nothing failed since', () => {
  expect(up(undefined)).toBe(false)
  expect(up({ ms: 12, peek: null })).toBe(true)
  expect(up({ peek })).toBe(true)
  expect(up({ ms: 12, error: refused as ConnError })).toBe(false)
  expect(up({ ms: null })).toBe(false)
})
