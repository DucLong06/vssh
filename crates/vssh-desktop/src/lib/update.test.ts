import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { setLang } from './i18n.svelte'
import Statusbar from './Statusbar.svelte'
import { check, newer, update } from './update.svelte'

/** Every command the window sent. */
let calls: string[] = []
/** GitHub, answering what a test says; never the network. */
const fetched = vi.fn<typeof fetch>()
const latest = (tag: unknown, ok = true) => fetched.mockResolvedValue({ ok, json: async () => ({ tag_name: tag }) } as Response)

beforeEach(() => {
  vi.stubGlobal('fetch', fetched)
  // This build is 0.2.0.
  mockIPC((cmd) => {
    calls.push(cmd)
    return cmd === 'plugin:app|version' ? '0.2.0' : null
  })
})
afterEach(() => {
  cleanup()
  clearMocks()
  vi.unstubAllGlobals()
  fetched.mockReset()
  update.tag = null
  calls = []
})

const bar = () => render(Statusbar, { hosts: 4, snippets: 1, sessions: 0, onsnippets: vi.fn() })

test('a version is newer by its numbers, with a v in front or without', () => {
  expect(newer('v0.2.1', '0.2.0')).toBe(true)
  expect(newer('v0.10.0', '0.9.9')).toBe(true)
  expect(newer('1.0', '0.9.9')).toBe(true)
  expect(newer('v0.2.0.1', '0.2.0')).toBe(true)
  expect(newer('v0.2.0', '0.2.0')).toBe(false)
  expect(newer('v0.2', '0.2.0')).toBe(false)
  expect(newer('v0.1.9', '0.2.0')).toBe(false)
})

test('a newer release: one line in the status bar, which opens the releases and downloads nothing', async () => {
  latest('v0.3.0')
  bar()
  expect(screen.queryByRole('status')).toBeNull()
  await check()
  expect(update.tag).toBe('v0.3.0')
  const line = await screen.findByRole('button', { name: 'New version v0.3.0 — see it on GitHub' })
  await userEvent.click(line)
  expect(calls).toEqual(['plugin:app|version', 'open_releases'])
  expect(fetched).toHaveBeenCalledOnce()
})

test('the question is one public GET: no header, no body, no cookie, nothing of the user in the address', async () => {
  latest('v0.3.0')
  await check()
  const [url, init] = fetched.mock.calls[0]
  expect(url).toBe('https://api.github.com/repos/DucLong06/vssh/releases/latest')
  const { signal, ...rest } = init!
  expect(signal).toBeInstanceOf(AbortSignal)
  expect(rest).toEqual({ credentials: 'omit', referrerPolicy: 'no-referrer', cache: 'no-store' })
})

test('no newer release: the same version, or an older one, says nothing', async () => {
  bar()
  for (const tag of ['v0.2.0', '0.2.0', 'v0.1.9']) {
    latest(tag)
    await check()
    expect(update.tag).toBeNull()
  }
  expect(fetched).toHaveBeenCalledTimes(3)
  expect(screen.queryByRole('status')).toBeNull()
  expect(screen.queryByRole('button', { name: /New version/ })).toBeNull()
})

test('no network, a dead proxy, a refusal, an answer that is no release: silence, and no rejection', async () => {
  bar()
  fetched.mockRejectedValue(new TypeError('Load failed'))
  await expect(check()).resolves.toBeUndefined()
  latest('v9.9.9', false)
  await expect(check()).resolves.toBeUndefined()
  fetched.mockResolvedValue({ ok: true, json: () => Promise.reject(new SyntaxError('not JSON')) } as unknown as Response)
  await expect(check()).resolves.toBeUndefined()
  // Only what looks like a version is put on screen.
  for (const tag of [undefined, 9, '', 'nightly', 'v1.0.0 <b>now</b>', 'v1.0.0-rc.1']) {
    latest(tag)
    await expect(check()).resolves.toBeUndefined()
  }
  expect(update.tag).toBeNull()
  expect(screen.queryByRole('status')).toBeNull()
  expect(screen.queryByRole('alert')).toBeNull()
  expect(calls).not.toContain('open_releases')
})

test('dismissed, the line is gone for as long as the app runs; a browser that does not open is no error', async () => {
  latest('v0.3.0')
  mockIPC((cmd) => {
    if (cmd === 'open_releases') throw { class: 'other', fail: null, chain: ['xdg-open could not open'] }
    return '0.2.0'
  })
  bar()
  await check()
  await userEvent.click(await screen.findByRole('button', { name: /New version v0.3.0/ }))
  expect(screen.queryByRole('alert')).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))
  expect(update.tag).toBeNull()
  expect(screen.queryByRole('button', { name: /New version/ })).toBeNull()
  expect(fetched).toHaveBeenCalledOnce()
})

test('in Vietnamese', async () => {
  setLang('vi')
  latest('v0.3.0')
  bar()
  await check()
  expect(await screen.findByRole('button', { name: 'Có bản mới v0.3.0 — xem trên GitHub' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Tắt thông báo' })).toBeInTheDocument()
})
