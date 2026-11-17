import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import type { ConnError, Host, Peek, Shown } from './api'
import Dashboard from './Dashboard.svelte'
import errors from './fixtures/errors.json'
import fixture from './fixtures/hosts.json'
import peekFixture from './fixtures/peek.json'
import shown from './fixtures/shown.json'
import { setLang } from './i18n.svelte'
import { forget, live } from './live.svelte'

const hosts = fixture as Host[]
const peek = peekFixture as unknown as Peek
/** What the shell was asked, and what it answers the next time. */
let asked: unknown[] = []
let answer: () => unknown = () => peek

const open = () => {
  render(Dashboard, { hosts, onconnect: vi.fn(), onpalette: vi.fn() })
  return userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
}
const card = (alias: string) => screen.getByRole('article', { name: alias })
const popover = () => screen.queryByRole('dialog')
/** The rows of the popover: label, then value. */
const rows = () =>
  within(popover()!)
    .getAllByRole('term')
    .map((term) => [term.textContent, term.nextElementSibling?.textContent?.replace(/\s+/g, ' ').trim()])

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  answer = () => peek
  mockIPC((cmd, args) => {
    if (cmd === 'resolve_conn_error') return shown.en[2]
    asked.push([cmd, args])
    return answer()
  })
})

afterEach(() => {
  vi.useRealTimers()
  clearMocks()
  forget()
  asked = []
})

test('v on a card opens the quick peek: everything the host said about itself', async () => {
  live['web-01'] = { ms: 12, peek, at: Date.now() }
  const user = open()
  expect(popover()).not.toBeInTheDocument()
  card('web-01').focus()
  await user.keyboard('v')

  expect(popover()).toHaveAccessibleName('Quick peek: web-01')
  expect(popover()).toHaveTextContent('CONNECTION')
  expect(popover()).toHaveTextContent('deploy@10.0.0.5:2222')
  expect(popover()).toHaveTextContent('reachable · 12 ms')
  expect(rows()).toEqual([
    ['Host', 'web-01'],
    ['System', 'Linux 6.14.0-37-generic x86_64'],
    ['Uptime', '3 days, 2:15'],
    ['Load', '0.92 0.58 0.59'],
    ['CPU', '23%'],
    ['MEM', '4.2/16G'],
    ['Disk /', '61%'],
  ])
  // It was answered a moment ago: nobody is asked again.
  expect(asked).toEqual([])
})

test('an answer is good for 30 seconds; after that, looking asks again, and r asks at once', async () => {
  live['web-01'] = { ms: 12, peek, at: Date.now() - 31_000 }
  const user = open()
  card('web-01').focus()
  await user.keyboard('v')
  expect(asked).toEqual([['get_host_stats', { alias: 'web-01', passive: false }]])
  await vi.waitFor(() => expect(popover()).toHaveTextContent('0s ago'))

  // Now it is fresh: closing and looking again asks nobody.
  await user.keyboard('{Escape}v')
  expect(asked).toHaveLength(1)
  answer = () => ({ ...peek, uptime: '3 days, 2:16' })
  await user.keyboard('r')
  expect(asked).toHaveLength(2)
  await vi.waitFor(() => expect(rows()[2]).toEqual(['Uptime', '3 days, 2:16']))
})

test('the age of the answer counts up while the popover is open', async () => {
  live['web-01'] = { ms: 12, peek, at: Date.now() - 4000 }
  const user = open()
  card('web-01').focus()
  await user.keyboard('v')
  expect(popover()).toHaveTextContent('4s ago')
  await vi.advanceTimersByTimeAsync(3000)
  expect(popover()).toHaveTextContent('7s ago')
})

test('Esc closes it, and so does moving on to another card', async () => {
  live['web-01'] = { ms: 12, peek, at: Date.now() }
  const user = open()
  card('web-01').focus()
  await user.keyboard('v')
  expect(popover()).toBeInTheDocument()
  await user.keyboard('{Escape}')
  expect(popover()).not.toBeInTheDocument()
  // The card keeps the focus: v opens it again.
  expect(card('web-01')).toHaveFocus()
  await user.keyboard('v')
  expect(popover()).toBeInTheDocument()
  card('web-02').focus()
  await vi.waitFor(() => expect(popover()).not.toBeInTheDocument())
})

test('a host that was not checked is asked when a person looks at it', async () => {
  live.bare = { peek: null, at: Date.now() }
  const user = open()
  card('bare').focus()
  await user.keyboard('v')
  expect(asked).toEqual([['get_host_stats', { alias: 'bare', passive: false }]])
  await vi.waitFor(() => expect(rows()[1]).toEqual(['System', 'Linux 6.14.0-37-generic x86_64']))
  // No ping was answered for it.
  expect(popover()).toHaveTextContent('not checked')
})

test('resting the pointer on a card for 600 ms opens it; leaving closes it; and that asks nobody', async () => {
  live.bare = { peek: null, at: Date.now() - 60_000 }
  const user = open()
  await user.hover(card('bare'))
  await vi.advanceTimersByTimeAsync(590)
  expect(popover()).not.toBeInTheDocument()
  await vi.advanceTimersByTimeAsync(20)
  expect(popover()).toHaveAccessibleName('Quick peek: bare')
  // Resting on a card is not a decision to connect to a host never seen.
  expect(asked).toEqual([])
  expect(popover()).toHaveTextContent('not checked')

  // Into the popover itself is not leaving.
  await user.hover(popover()!)
  expect(popover()).toBeInTheDocument()
  await user.unhover(card('bare'))
  await user.hover(card('web-01'))
  expect(popover()).not.toBeInTheDocument()
  // Passing over is not resting.
  await vi.advanceTimersByTimeAsync(300)
  await user.unhover(card('web-01'))
  await vi.advanceTimersByTimeAsync(1000)
  expect(popover()).not.toBeInTheDocument()
})

test('r in a popover the pointer opened is a person asking', async () => {
  live.bare = { peek: null, at: Date.now() }
  const user = open()
  await user.hover(card('bare'))
  await vi.advanceTimersByTimeAsync(600)
  await user.keyboard('r')
  expect(asked).toEqual([['get_host_stats', { alias: 'bare', passive: false }]])
})

test('a host that is down: its failure, the whole chain, and r to try again', async () => {
  live['web-02'] = { ms: null, error: errors[2] as ConnError, shown: shown.en[2] as Shown, at: Date.now() }
  const user = open()
  card('web-02').focus()
  await user.keyboard('v')
  expect(popover()).toHaveTextContent('unreachable')
  expect(popover()).toHaveTextContent('✗ REFUSED Connection refused by 10.0.0.5:2222 — is sshd running?')
  expect(popover()).toHaveTextContent('connection refused by 10.0.0.5:2222: Connection refused (os error 111)')
  expect(popover()).toHaveTextContent('r try again')
  expect(within(popover()!).queryAllByRole('term')).toHaveLength(0)

  await user.keyboard('r')
  expect(asked).toHaveLength(1)
  await vi.waitFor(() => expect(rows()).toHaveLength(7))
})

test('what the host did not say is a dash, never a guess', async () => {
  const quiet = { stats: { cpu: null, mem: null, disk: 84 }, load: null, uptime: null, os: 'Linux 5.10 aarch64', host: null }
  live['db-primary'] = { peek: quiet, at: Date.now() }
  const user = open()
  card('db-primary').focus()
  await user.keyboard('v')
  expect(rows()).toEqual([
    ['Host', '—'],
    ['System', 'Linux 5.10 aarch64'],
    ['Uptime', '—'],
    ['Load', '—'],
    ['CPU', '—'],
    ['MEM', '—'],
    ['Disk /', '84%'],
  ])
  expect(popover()!.textContent).not.toMatch(/null|undefined|NaN/)
})

test('before the answer: asking; and typing in the search box opens nothing', async () => {
  answer = () => new Promise(() => {})
  const user = open()
  card('web-02').focus()
  await user.keyboard('v')
  expect(popover()).toHaveTextContent('asking web-02…')
  await user.keyboard('{Escape}')
  await user.keyboard('/vr')
  expect(screen.getByRole('searchbox')).toHaveValue('vr')
  expect(popover()).not.toBeInTheDocument()
})

test("in Vietnamese the rows are the TUI's", async () => {
  setLang('vi')
  live['web-01'] = { ms: 12, peek, at: Date.now() - 2000 }
  const user = open()
  card('web-01').focus()
  await user.keyboard('v')
  expect(popover()).toHaveAccessibleName('Xem nhanh: web-01')
  expect(popover()).toHaveTextContent('KẾT NỐI')
  expect(popover()).toHaveTextContent('kết nối được · 12 ms')
  expect(rows().map((row) => row[0])).toEqual(['Tên máy', 'Hệ thống', 'Đã chạy', 'Tải', 'CPU', 'RAM', 'Đĩa /'])
  expect(popover()).toHaveTextContent('2 giây trước')
  expect(popover()).toHaveTextContent('r làm mới')
  expect(popover()).toHaveTextContent('esc đóng')
})
