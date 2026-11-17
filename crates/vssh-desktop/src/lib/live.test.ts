import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { afterEach, expect, test, vi } from 'vitest'
import type { Peek } from './api'
import errors from './fixtures/errors.json'
import fixture from './fixtures/peek.json'
import shown from './fixtures/shown.json'
import { ask, forget, live, ping, poll, relabel, sick } from './live.svelte'

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
