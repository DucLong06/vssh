import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.svelte'
import type { Host } from './lib/api'
import listing from '../src-tauri/tests/fixtures/files.json'
import errors from './lib/fixtures/errors.json'
import forwards from './lib/fixtures/forwards.json'
import hosts from './lib/fixtures/hosts.json'
import peek from './lib/fixtures/peek.json'
import refused from './lib/fixtures/refused.json'
import shown from './lib/fixtures/shown.json'
import { terminals } from './lib/fixtures/xterm'
import { setLang } from './lib/i18n.svelte'
import { forget } from './lib/live.svelte'
import { capital, sheet } from './lib/keymap'
import { tunnels } from './lib/tunnels.svelte'
import { fake, setStreamer } from './lib/streamer.svelte'
import strings from './lib/strings.json'

type Args = Record<string, unknown>
const snippets = [{ id: 1, host: '', name: 'up', command: 'uptime' }]
/** Every command the window sent, with what it sent. */
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])

/** The shell, answering like the real one; `over` replaces single commands. */
function shell(over: Record<string, (args: Args) => unknown> = {}) {
  let sessions = 0
  const commands: Record<string, (args: Args) => unknown> = {
    list_hosts: () => hosts,
    list_snippets: () => snippets,
    get_host_stats: () => peek,
    ping_host: () => 12,
    session_open: () => ++sessions,
    quick_connect: () => ++sessions,
    session_start: () => null,
    session_write: () => null,
    session_resize: () => null,
    session_close: () => null,
    session_trust: () => null,
    'plugin:clipboard-manager|write_text': () => null,
    local_list: () => ({ path: '/home/me', entries: [] }),
    sftp_realpath: () => '/home/deploy',
    sftp_list: () => listing,
    vault_state: () => 'none',
    vault_hosts: () => [],
    // The core's own words for a failure of that class.
    resolve_conn_error: ({ error, lang }) =>
      shown[lang as 'en' | 'vi'][errors.findIndex((known) => known.class === (error as typeof refused).class)],
    ...over,
  }
  mockIPC(
    (cmd, args) => {
      calls.push([cmd, args as Args])
      return commands[cmd](args as Args)
    },
    { shouldMockEvents: true },
  )
  render(App)
  // Under fake timers the user's own pauses are on the fake clock too.
  return userEvent.setup({ advanceTimers: (ms) => void (vi.isFakeTimers() && vi.advanceTimersByTime(ms)) })
}
const status = () => screen.getByRole('contentinfo')
/** The palette's box. Behind its dialog the page is inert; jsdom does not know. */
const palette = () => within(screen.getByRole('dialog')).getByRole('combobox')
const card = (alias: string) => screen.findByRole('article', { name: alias })
/** A row of the settings at the rail's foot, chosen: the language, the theme, streamer mode, focus mode, the cheatsheet. */
const setting = async (user: ReturnType<typeof userEvent.setup>, name: RegExp) => {
  await user.click(screen.getByRole('button', { name: /^(Settings|Cài đặt)$/ }))
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name }))
}
const fails = () => {
  throw refused
}
const tab = (name: RegExp) => screen.findByRole('tab', { name })
/** What a session's shell prints, and what it says about itself. */
const prints = (id: number, text: string) => emit(`pty-data:${id}`, [...new TextEncoder().encode(text)])
const says = (id: number, state: object) => emit(`pty-state:${id}`, state)
const copied = () => asked('plugin:clipboard-manager|write_text').map((args) => args.text)
const leader = '{Control>}a{/Control}'
/** The window's own chords, which work in a terminal too: close the tab, the palette. */
const ctrlW = '{Control>}{Shift>}W{/Shift}{/Control}'
const ctrlK = '{Control>}{Shift>}K{/Shift}{/Control}'

afterEach(async () => {
  vi.useRealTimers()
  // The terminals stop listening when they go, which takes the mocks: first
  // the window, then a turn of the loop, then the mocks.
  cleanup()
  await new Promise((done) => setTimeout(done))
  terminals.length = 0
  clearMocks()
  localStorage.clear()
  forget()
  tunnels.clear()
  calls = []
  put = []
})

test('it opens on the dashboard: a card for every host of the config, with its figures', async () => {
  shell()
  expect(await card('db-primary')).toBeInTheDocument()
  expect(screen.getAllByRole('article')).toHaveLength(4)
  await vi.waitFor(async () => expect(await card('web-01')).toHaveTextContent('CPU 23%'))
  expect(await card('web-01')).toHaveTextContent('12 ms')
  expect(status()).toHaveTextContent('Hosts 4')
  expect(status()).toHaveTextContent('Snippets 1')
  expect(status()).toHaveTextContent('Sessions 0')
})

test('every five seconds each host is asked again, on the dashboard\'s own account; pings are rarer', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  shell()
  await card('bare')
  await vi.waitFor(() => expect(asked('get_host_stats')).toHaveLength(4))
  expect(asked('get_host_stats')[0]).toEqual({ alias: 'web-01', passive: true })
  expect(asked('ping_host')).toHaveLength(4)

  await vi.advanceTimersByTimeAsync(5000)
  expect(asked('get_host_stats')).toHaveLength(8)
  expect(asked('get_host_stats').every((args) => args.passive === true)).toBe(true)
  expect(asked('ping_host')).toHaveLength(4)
  // A minute after the first: the twelfth round since.
  await vi.advanceTimersByTimeAsync(55_000)
  expect(asked('ping_host')).toHaveLength(8)
})

test('a host that does not answer: its card says which kind of failure, in the language of the window', async () => {
  const user = shell({
    get_host_stats: ({ alias }) => {
      if (alias === 'web-02') throw refused
      return peek
    },
  })
  const down = await card('web-02')
  await vi.waitFor(() => expect(down).toHaveTextContent('✗ REFUSED Connection refused by 10.0.0.5:2222'))
  expect(down).toHaveAttribute('data-state', 'down')
  expect(await card('web-01')).toHaveAttribute('data-state', 'ok')

  await setting(user, /Language/)
  await vi.waitFor(() => expect(down).toHaveTextContent('✗ BỊ TỪ CHỐI 10.0.0.5:2222 từ chối kết nối'))
})

// --- D3: terminal tabs -------------------------------------------------------

test('Connect on a card opens a tab with a terminal: what the shell prints goes in, keys and size go out', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  expect(await tab(/web-01/)).toHaveAttribute('aria-selected', 'true')
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }])
  // It connects once its terminal listens, at the terminal's size.
  await vi.waitFor(() => expect(asked('session_start')).toEqual([{ id: 1, cols: 80, rows: 24 }]))
  const [term] = terminals
  expect(term.focused).toBe(true)
  expect(term.options).toMatchObject({ fontFamily: expect.stringContaining('Maple Mono NF') })
  expect(term.unicode.activeVersion).toBe('11')
  expect(within(await tab(/web-01/)).getByRole('img', { name: 'connecting' })).toBeInTheDocument()

  await says(1, { state: 'up' })
  await prints(1, 'web:~# ')
  expect(term.written).toBe('web:~# ')
  expect(within(await tab(/web-01/)).getByRole('img', { name: 'connected' })).toBeInTheDocument()
  term.type('l')
  term.type('s\r')
  expect(asked('session_write')).toEqual([
    { id: 1, data: 'l' },
    { id: 1, data: 's\r' },
  ])
  term.resize(120, 40)
  expect(asked('session_resize')).toEqual([{ id: 1, cols: 120, rows: 40 }])
  expect(status()).toHaveTextContent('Sessions 1')
  // The dashboard is behind it, out of reach.
  expect(screen.queryByRole('article')).not.toBeInTheDocument()

  await user.click(screen.getByRole('button', { name: 'close web-01' }))
  expect(asked('session_close')).toEqual([{ id: 1 }])
  expect(screen.queryByRole('tab')).not.toBeInTheDocument()
  expect(terminals).toHaveLength(0)
  expect(screen.getAllByRole('article')).toHaveLength(4)
  expect(status()).toHaveTextContent('Sessions 0')
})

test('Ctrl-A is the leader, as in the TUI: digits and Tab change tab, a second Ctrl-A is for the shell', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/web-01/)
  const picked = () => screen.getByRole('tab', { selected: true })
  // The status bar lists no keys. A tab says its own in its tooltip: its number, after the leader.
  expect(status().querySelector('kbd')).toBeNull()
  expect(await tab(/web-01/)).toHaveAttribute('title', 'web-01 (Ctrl+A 1)')
  expect(screen.getByRole('tab', { name: /Hosts/ })).toHaveAttribute('title', 'Hosts (Ctrl+A 0)')
  // Its key closes the tab in front: its × says so, and no other does.
  expect(screen.getByRole('button', { name: 'close web-01' })).toHaveAttribute('title', 'Close (Ctrl+Shift+W)')
  await user.keyboard(`${leader}0`)
  expect(picked()).toHaveTextContent('0 Hosts')
  await user.click(within(await card('web-02')).getByRole('button', { name: 'Connect' }))
  expect(picked()).toHaveTextContent('2 web-02')
  expect(terminals[1].focused).toBe(true)

  await user.keyboard(`${leader}1`)
  expect(picked()).toHaveTextContent('1 web-01')
  expect(terminals[0].focused).toBe(true)
  expect(screen.getByRole('tabpanel', { name: 'web-01' })).toBeVisible()
  await user.keyboard(`${leader}{Tab}`)
  expect(picked()).toHaveTextContent('2 web-02')
  await user.keyboard(`${leader}n`)
  expect(picked()).toHaveTextContent('0 Hosts')
  await user.keyboard(`${leader}p`)
  expect(picked()).toHaveTextContent('2 web-02')
  // A host chosen in the palette is a card: the dashboard comes forward for it.
  await user.keyboard(ctrlK)
  await user.type(palette(), 'db{Enter}')
  expect(picked()).toHaveTextContent('0 Hosts')
  await vi.waitFor(async () => expect(await card('db-primary')).toHaveFocus())
  // Chosen again from a terminal, the same host has its card focused again.
  await user.keyboard(`${leader}2${ctrlK}`)
  await user.type(palette(), 'db{Enter}')
  expect(picked()).toHaveTextContent('0 Hosts')
  await vi.waitFor(async () => expect(await card('db-primary')).toHaveFocus())
  await user.keyboard(`${leader}2`)
  // A tab that is not there, a key that means nothing: the leader is over.
  await user.keyboard(`${leader}7${leader}{Escape}`)
  expect(picked()).toHaveTextContent('2 web-02')

  // The keys the window keeps reach the shell after the leader.
  await user.keyboard(`${leader}${leader}${leader}{Control>}w{/Control}`)
  expect(asked('session_write')).toEqual([
    { id: 2, data: '\x01' },
    { id: 2, data: '\x17' },
  ])
  // On their own, in a terminal, Ctrl-W and Ctrl-K are the shell's: a word is deleted there, the line cut, and no tab closed.
  await user.keyboard('{Control>}w{/Control}{Control>}k{/Control}')
  expect(asked('session_close')).toEqual([])
  expect(screen.queryByRole('dialog')).toBeNull()
  // With Shift they are the window's; Ctrl-A x is the TUI's key for the same.
  await user.keyboard(ctrlW)
  expect(asked('session_close')).toEqual([{ id: 2 }])
  expect(picked()).toHaveTextContent('1 web-01')
  await user.keyboard(`${leader}x`)
  expect(asked('session_close')).toEqual([{ id: 2 }, { id: 1 }])
  expect(screen.queryByRole('tablist')).not.toBeInTheDocument()
})

test('text leaves a terminal for the clipboard three ways: Ctrl-A y, a selection, and an OSC 52 from over there', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await vi.waitFor(() => expect(asked('session_start')).toHaveLength(1))
  const [term] = terminals
  await prints(1, 'web:~# ls\r\nold  news\r\nweb:~# df -h /\r\nFilesystem  Use%\r\n/dev/sda1   61%\r\nweb:~# ')
  await user.keyboard(`${leader}y`)
  expect(copied()).toEqual(['Filesystem  Use%\n/dev/sda1   61%'])
  // No modifier and no menu: what the mouse selects is copied.
  term.select('/dev/sda1')
  term.select('')
  expect(copied()).toHaveLength(2)
  expect(copied()[1]).toBe('/dev/sda1')
  // A program on the host copies; it cannot read what is there.
  expect(term.osc.get(52)?.(`c;${btoa('from vim')}`)).toBe(true)
  term.osc.get(52)?.('c;?')
  expect(copied()).toEqual(['Filesystem  Use%\n/dev/sda1   61%', '/dev/sda1', 'from vim'])
  // Nothing of this was typed into the shell.
  expect(asked('session_write')).toEqual([])
})

test('several marked cards open a tab each; the sidebar lists the sessions, and a busy one says what it runs', async () => {
  const user = shell()
  for (const alias of ['web-01', 'db-primary']) {
    ;(await card(alias)).focus()
    await user.keyboard(' ')
  }
  await user.keyboard('{Enter}')
  await vi.waitFor(() => expect(screen.getAllByRole('tab')).toHaveLength(3))
  const opened = asked('session_open').map((args) => args.alias as string)
  expect([...opened].sort()).toEqual(['db-primary', 'web-01'])
  // Each connects, in front or not.
  await vi.waitFor(() => expect(asked('session_start')).toHaveLength(2))
  expect(status()).toHaveTextContent('Sessions 2')
  // The last one opened is in front.
  expect(screen.getByRole('tab', { selected: true })).toHaveTextContent(`2 ${opened[1]}`)

  const rows = within(screen.getByRole('list', { name: 'Sessions' })).getAllByRole('button')
  expect(rows.map((row) => row.textContent?.replace(/\s+/g, ' ').trim())).toEqual(opened.map((alias) => `○ ${alias}`))
  await user.click(rows[0])
  const front = screen.getByRole('tab', { selected: true })
  expect(front).toHaveTextContent(`1 ${opened[0]}`)
  expect(rows[0]).toHaveAttribute('aria-current', 'true')
  expect(screen.getByRole('tabpanel', { name: opened[0] })).toBeVisible()

  await says(1, { state: 'up' })
  await says(1, { state: 'busy', command: 'sleep' })
  expect(front).toHaveTextContent(`● 1 ${opened[0]} running sleep`)
  await says(1, { state: 'busy', command: null })
  expect(front).not.toHaveTextContent('running')
  // A shell that ended runs nothing.
  await says(1, { state: 'busy', command: 'top' })
  await says(1, { state: 'closed', error: null })
  expect(front).not.toHaveTextContent('running')
})

// --- M3: broadcast, and failures that ask ------------------------------------

/** Opens a tab on a host from wherever the window is, and waits for its terminal to connect. */
async function connected(user: ReturnType<typeof shell>, alias: string) {
  const before = asked('session_start').length
  await user.keyboard(`${leader}0`)
  await user.click(within(await card(alias)).getByRole('button', { name: 'Connect' }))
  await vi.waitFor(() => expect(asked('session_start')).toHaveLength(before + 1))
}
const typed = () => asked('session_write').map((args) => `${args.id}:${args.data}`)

test('broadcast: what is typed in one broadcasting tab goes to every broadcasting tab, and to no other', async () => {
  const user = shell()
  for (const alias of ['web-01', 'web-02', 'db-primary']) await connected(user, alias)
  for (const id of [1, 2, 3]) await says(id, { state: 'up' })
  expect(status()).not.toHaveTextContent('Broadcasting')

  // Tab 3 is in front; then tab 1.
  await user.keyboard(`${leader}b${leader}1${leader}b`)
  expect(status()).toHaveTextContent('Broadcasting: 2')
  const dots = () => screen.getAllByRole('tab').slice(1).map((el) => within(el).getByRole('img').getAttribute('aria-label'))
  expect(dots()).toEqual(['broadcast', 'connected', 'broadcast'])
  expect((await tab(/web-01/)).parentElement).toHaveAttribute('data-broadcast')
  expect((await tab(/web-02/)).parentElement).not.toHaveAttribute('data-broadcast')

  terminals[0].type('uptime\r')
  expect(typed()).toEqual(['1:uptime\r', '3:uptime\r'])
  // A tab that is not broadcasting hears nothing of it, and speaks for itself only.
  terminals[1].type('ls\r')
  expect(typed().slice(2)).toEqual(['2:ls\r'])
  // So do the keys after the leader.
  await user.keyboard(`${leader}${leader}`)
  expect(typed().slice(3)).toEqual(['1:\x01', '3:\x01'])

  // A broadcasting tab that died is red, not yellow.
  await says(3, { state: 'closed', error: null })
  expect(dots()).toEqual(['broadcast', 'connected', 'disconnected'])

  // Off for this tab; Ctrl-A B is every tab at once, and off again.
  await user.keyboard(`${leader}b`)
  expect(status()).toHaveTextContent('Broadcasting: 1')
  terminals[0].type('x')
  expect(typed().slice(5)).toEqual(['1:x'])
  await user.keyboard(`${leader}B`)
  expect(status()).toHaveTextContent('Broadcasting: 3')
  await user.keyboard(`${leader}B`)
  expect(status()).not.toHaveTextContent('Broadcasting')
})

test('a connection that fails at a jump host says which hop and why, in a dialog; a drop later does not', async () => {
  const user = shell()
  await connected(user, 'web-01')
  await connected(user, 'web-02')
  const [hostKey, jump] = [errors[5], errors[6]]
  // The tab behind fails: it comes to the front with its reason.
  await says(1, { state: 'closed', error: jump })
  const dialog = await screen.findByRole('dialog', { name: 'JUMP HOST: web-01' })
  expect(dialog).toHaveTextContent('Jump host bastion failed: Connection refused by 192.0.2.1:22 — is sshd running?')
  expect(dialog).toHaveTextContent('jump host bastion failed: connection refused by 192.0.2.1:22: Connection refused (os error 111)')
  expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('1 web-01')
  // The leader is not for a dialog.
  await user.keyboard(`${leader}2`)
  expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('1 web-01')

  await user.click(within(dialog).getByRole('button', { name: 'try again' }))
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(asked('session_start').map((args) => args.id)).toEqual([1, 2, 1])
  // Cancel leaves the tab as it is: closed, with its reason and a way back.
  await says(1, { state: 'closed', error: jump })
  await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }))
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(within(screen.getByRole('tabpanel', { name: 'web-01' })).getByRole('status')).toHaveTextContent('JUMP HOST')

  // A session that was up and lost its connection keeps its screen: no dialog.
  await says(2, { state: 'up' })
  await says(2, { state: 'closed', error: hostKey })
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  await user.keyboard(`${leader}2`)
  const badge = within(screen.getByRole('tabpanel', { name: 'web-02' })).getByRole('status')
  await vi.waitFor(() => expect(badge).toHaveTextContent('HOST KEY'))
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})

test('a host key that is not the one on record stops the session and asks: accept once, trust, or cancel', async () => {
  const user = shell()
  await connected(user, 'web-01')
  const hostKey = errors[5]
  const fail = hostKey.fail as { host: string; fingerprint: string }
  const ask = async () => {
    await says(1, { state: 'closed', error: hostKey })
    return screen.findByRole('dialog', { name: 'HOST KEY: web-01' })
  }
  const dialog = await ask()
  expect(dialog).toHaveTextContent('Host key mismatch for 10.0.0.5')
  expect(dialog).toHaveTextContent(`Fingerprint ${fail.fingerprint}`)
  expect(dialog).toHaveTextContent('On record /home/me/.ssh/known_hosts:7')
  expect(dialog).toHaveTextContent('Accept only if you know why its key changed')
  // There is no trying again without an answer.
  expect(within(dialog).queryByRole('button', { name: 'try again' })).not.toBeInTheDocument()

  await user.click(within(dialog).getByRole('button', { name: 'Accept once' }))
  const yes = { id: 1, host: '10.0.0.5', fingerprint: fail.fingerprint }
  await vi.waitFor(() => expect(asked('session_start')).toHaveLength(2))
  expect(asked('session_trust')).toEqual([{ ...yes, forever: false }])
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()

  await user.click(within(await ask()).getByRole('button', { name: 'Trust' }))
  await vi.waitFor(() => expect(asked('session_start')).toHaveLength(3))
  expect(asked('session_trust')[1]).toEqual({ ...yes, forever: true })

  await user.click(within(await ask()).getByRole('button', { name: 'Cancel' }))
  expect(asked('session_trust')).toHaveLength(2)
  expect(asked('session_start')).toHaveLength(3)

  // The key of a jump host on the way: the yes is for that host.
  await user.click(screen.getByRole('button', { name: 'Reconnect' }))
  const bastion = { ...fail, kind: 'host_key', host: 'bastion.example.com' }
  await says(1, { state: 'closed', error: { class: 'jump', fail: { kind: 'jump', hop: 'bastion', cause: bastion }, chain: [] } })
  await user.click(within(await screen.findByRole('dialog', { name: 'JUMP HOST: web-01' })).getByRole('button', { name: 'Trust' }))
  await vi.waitFor(() => expect(asked('session_trust')).toHaveLength(3))
  expect(asked('session_trust')[2]).toEqual({ ...yes, host: 'bastion.example.com', forever: true })
})

test('a shell that ends leaves its tab red with a way back: reconnect starts the same session again', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await vi.waitFor(() => expect(asked('session_start')).toHaveLength(1))
  await says(1, { state: 'up' })
  expect(screen.queryByRole('button', { name: 'Reconnect' })).not.toBeInTheDocument()

  // The network went away under it.
  await says(1, { state: 'closed', error: refused })
  const badge = await within(screen.getByRole('tabpanel', { name: 'web-01' })).findByRole('status')
  await vi.waitFor(() => expect(badge).toHaveTextContent('disconnected REFUSED Connection refused by 10.0.0.5:2222'))
  expect(within(await tab(/web-01/)).getByRole('img', { name: 'disconnected' })).toBeInTheDocument()
  // In the language of the window.
  await setting(user, /Language/)
  await vi.waitFor(() => expect(badge).toHaveTextContent('mất kết nối BỊ TỪ CHỐI 10.0.0.5:2222 từ chối kết nối'))

  terminals[0].resize(100, 30)
  await user.click(within(badge).getByRole('button', { name: 'Kết nối lại' }))
  expect(asked('session_start')).toEqual([
    { id: 1, cols: 80, rows: 24 },
    { id: 1, cols: 100, rows: 30 },
  ])
  expect(screen.queryByRole('button', { name: 'Kết nối lại' })).not.toBeInTheDocument()
  expect(within(await tab(/web-01/)).getByRole('img', { name: 'đang kết nối' })).toBeInTheDocument()
  // A clean end (`exit`) says so too, with nothing to explain.
  await says(1, { state: 'up' })
  await says(1, { state: 'closed', error: null })
  expect(await screen.findByRole('button', { name: 'Kết nối lại' })).toBeInTheDocument()
})

test('a change made elsewhere, to the config or by the TUI, shows up on its own', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  let list = hosts
  shell({ list_hosts: () => list })
  await card('bare')
  expect(screen.queryByRole('heading', { name: '#staging 2' })).not.toBeInTheDocument()

  const moved = { ...hosts[3], meta: { ...hosts[3].meta, grp: 'staging' } }
  const added = { ...hosts[3], alias: 'cache-01', meta: { ...hosts[3].meta, tags: 'staging' } }
  list = [hosts[0], hosts[1], moved, added]
  await vi.advanceTimersByTimeAsync(5000)
  expect(await screen.findByRole('heading', { name: '#staging 2' })).toBeInTheDocument()
  expect(screen.queryByRole('article', { name: 'db-primary' })).not.toBeInTheDocument()
  expect(status()).toHaveTextContent('Hosts 4')
  // The new host is asked like the others from the next round on.
  expect(asked('get_host_stats').at(-1)).toEqual({ alias: 'cache-01', passive: true })
})

test('a tag clicked in the sidebar filters the dashboard, and a second click lets go', async () => {
  const user = shell()
  await card('bare')
  await user.click(screen.getByRole('button', { name: '#db 1' }))
  expect(screen.getAllByRole('article').map((el) => el.getAttribute('aria-label'))).toEqual(['db-primary'])
  await user.click(screen.getByRole('button', { name: '#db 1' }))
  expect(screen.getAllByRole('article')).toHaveLength(4)
})

test('with no hosts at all it still opens, and says what to do', async () => {
  shell({ list_hosts: () => [], list_snippets: () => [] })
  expect(await screen.findByRole('heading', { name: 'No hosts yet.' })).toBeInTheDocument()
  expect(status()).toHaveTextContent('Hosts 0')
})

test('hosts that cannot be read: the reason is on screen', async () => {
  shell({ list_hosts: fails })
  const alert = await screen.findByRole('alert')
  expect(alert).toHaveTextContent('REFUSED Connection refused by 10.0.0.5:2222 — is sshd running?')
})

test('Ctrl-K opens the palette; Ctrl-A is the leader and waits for its key', async () => {
  const user = shell()
  await screen.findByRole('button', { name: /db-primary/ })
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  await user.keyboard('{Control>}k{/Control}')
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await user.keyboard(leader)
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  // Half a chord: the status bar says so until the second key comes.
  expect(within(status()).getByRole('status')).toHaveTextContent('Ctrl+A waiting for the second key — ? lists them')
  await user.keyboard('{Escape}')
  expect(within(status()).queryByRole('status')).not.toBeInTheDocument()
  // In a field Ctrl-A is the field's: it selects the text.
  await user.type(screen.getByRole('searchbox'), `web${leader}`)
  expect(within(status()).queryByRole('status')).not.toBeInTheDocument()
})

test('a host chosen in the palette is the selected one', async () => {
  const user = shell()
  await screen.findByRole('button', { name: /db-primary/ })
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'db{Enter}')
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: /db-primary/ })).toHaveAttribute('aria-current', 'true')
  expect(status()).toHaveTextContent('db-primary postgres@db.internal.example.com')
  await vi.waitFor(async () => expect(await card('db-primary')).toHaveFocus())
  // No row of keys under the window: each of the card's buttons says its own, in the window's language.
  expect(status().querySelector('kbd')).toBeNull()
  const titles = async () => within(await card('db-primary')).getAllByRole('button').map((button) => button.title)
  expect(await titles()).toEqual(['Edit (E)', 'Forwards (L)', 'Files (F)', 'Connect (Enter)'])
  await setting(user, /Language/)
  expect(await titles()).toEqual(['Sửa (E)', 'Chuyển tiếp (L)', 'Tệp (F)', 'Kết nối (Enter)'])
  // Chosen again from where the focus went (the button): the card has it back, for its keys.
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'db{Enter}')
  await vi.waitFor(async () => expect(await card('db-primary')).toHaveFocus())
})

test('the selected host is scrolled back into view when the sessions take room from the sidebar\'s list', async () => {
  const user = shell()
  const row = await screen.findByRole('button', { name: /db-primary/ })
  await user.click(row)
  const scrolled = vi.spyOn(Element.prototype, 'scrollIntoView')
  // Its own card: the selection does not change, only the list's height.
  await user.click(within(await card('db-primary')).getByRole('button', { name: 'Connect' }))
  await tab(/db-primary/)
  expect(scrolled.mock.contexts).toContain(row)
  scrolled.mockRestore()
})

test('quick connect from the palette opens a session and saves no host', async () => {
  const sent: unknown[] = []
  const user = shell({
    quick_connect: (args) => {
      sent.push(args)
      return 7
    },
  })
  await screen.findByRole('button', { name: /db-primary/ })
  await user.keyboard('{Control>}k{/Control}')
  // No host answers to it: it is an address, connected to as it stands.
  await user.type(palette(), 'deploy@10.0.0.9')
  expect(screen.getByRole('option', { name: /Connect to deploy@10.0.0.9/ })).toHaveAttribute('aria-selected', 'true')
  await user.keyboard('{Enter}')

  expect(await tab(/deploy@10.0.0.9/)).toHaveAttribute('aria-selected', 'true')
  await vi.waitFor(() => expect(asked('session_start')).toEqual([{ id: 7, cols: 80, rows: 24 }]))
  expect(sent).toEqual([{ host: 'deploy@10.0.0.9' }])
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(status()).toHaveTextContent('Sessions 1')
  expect(status()).toHaveTextContent('Hosts 4')
})

test('a refused quick connect says why, in the language of the window', async () => {
  const user = shell({ quick_connect: fails })
  await card('web-01')
  await setting(user, /Language/)
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), '10.0.0.9{Enter}')
  expect(await screen.findByRole('alert')).toHaveTextContent('BỊ TỪ CHỐI 10.0.0.5:2222 từ chối kết nối')
  // The palette stays, with what was typed: another try is one key away.
  expect(screen.getByRole('dialog', { name: 'Bảng lệnh' })).toBeInTheDocument()
  expect(palette()).toHaveValue('10.0.0.9')
  expect(stray()).toEqual([])
  await shut()
  expect(status()).toHaveTextContent('Phiên 0')
})

test('the language toggle changes every word and is remembered', async () => {
  const user = shell()
  await card('web-01')
  await setting(user, /Language/)
  expect(screen.getByRole('heading', { name: 'Máy chủ 4' })).toBeInTheDocument()
  expect(screen.getAllByRole('button', { name: 'Kết nối' })).toHaveLength(4)
  expect(status()).toHaveTextContent('Phiên 0')
  expect(localStorage.getItem('vssh.lang')).toBe('vi')
  expect(document.documentElement.lang).toBe('vi')
  await setting(user, /Ngôn ngữ/)
  expect(screen.getByRole('heading', { name: 'Hosts 4' })).toBeInTheDocument()
})

// --- M4: Vietnamese, with nothing left in English ---------------------------

/** Everything a person can read or have read to them: text, and the attributes that carry words. */
function words(): string {
  const said = [document.body.textContent ?? '']
  for (const el of document.body.querySelectorAll('*')) {
    for (const name of ['aria-label', 'placeholder', 'title', 'alt']) said.push(el.getAttribute(name) ?? '')
  }
  return said.join('\n')
}

/** The English strings of the table that show up in `text`, as whole words. */
function english(text: string): string[] {
  const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')
  return Object.entries(strings.en)
    .filter(([key, en]) => en !== strings.vi[key as keyof typeof strings.vi])
    .filter(([, en]) => {
      // `{n}` stands for a number the window puts in.
      const pattern = escape(en).replace('\\{n\\}', '\\d+')
      return new RegExp(`(?<![\\p{L}\\p{N}])${pattern}(?![\\p{L}\\p{N}])`, 'u').test(text)
    })
    .map(([key]) => key)
}

/** What the fixtures say: names, addresses, what a host printed, the core's words for a failure. Data, in no language. */
const DATA = [
  ...hosts.flatMap((host) => [host.alias, host.hostname ?? '', host.user ?? '', host.meta.grp, host.meta.notes, ...host.meta.tags.split(/[ ,]/)]),
  ...[peek.load, peek.os, peek.host, ...peek.uptime.split(/[\d\s,:]+/)],
  ...shown.vi.flatMap((said) => [said.label, said.message, said.chain]),
  ...errors.flatMap(({ fail }) => (fail && 'fingerprint' in fail ? [String(fail.fingerprint), String(fail.file)] : [])),
  ...forwards.flatMap((row) => [row.host, row.value]),
  ...listing.map((entry) => entry.name),
  ...snippets.flatMap((snippet) => [snippet.name, snippet.command]),
  // Paths, whole and as the file panes' crumbs have them.
  ...['/home/me', '/home/deploy', 'home', 'me', 'deploy'],
]
/**
 * What is the same in every language: the names of keys, units, the two
 * languages' own codes, the shapes of what is typed. The units of an uptime
 * are the core's (`vssh_core::stats::uptime`), as `stats.ts` has them.
 */
const KEPT = [
  ...['Ctrl', 'Shift', 'Alt', 'Tab', 'Enter', 'Space', 'Esc', 'Home', 'End', 'Backspace', 'Del', 'CPU', 'ms', 'EN', 'VI'],
  // The brand mark in the rail: a > whose command is ssh, drawn, not said in any language.
  'ssh',
  ...['ngày', 'giờ', 'phút', 'giây'],
  ...['user@host[:port]', '~/.ssh/config', '~/.ssh/id_ed25519', 'localhost:80', 'SOCKS', 'systemctl restart {{unit}}'],
]

/**
 * The words of `text` that are neither the Vietnamese table's, nor data, nor
 * the same in every language: text some component has of its own. `typed`:
 * what the test itself put in.
 */
function foreign(text: string, typed: string[] = []): string[] {
  // `{n}` and `{name}` are what the window puts into a text of the table: its pieces are what is looked for.
  // Also in small letters, and with a capital: a tooltip starts with one, whatever the table's word has.
  const table = Object.values(strings.vi).flatMap((vi) => vi.split(/\{\w+\}/)).flatMap((piece) => [piece, piece.toLowerCase(), capital(piece)])
  const known = [...table, ...DATA, ...typed, ...KEPT].map((piece) => piece.trim()).filter((piece) => piece.length > 1)
  let left = text
  // The longest first: `Máy chủ` before `Máy`.
  for (const piece of known.sort((a, b) => b.length - a.length)) left = left.replaceAll(piece, ' ')
  return [...new Set(left.match(/\p{L}[\p{L}\p{N}'’-]*\p{L}/gu) ?? [])]
}

/** What a test itself put on screen: a command, a file's name. */
let put: string[] = []
/** Everything on screen that is not Vietnamese: English of the table, and words that are in no table at all. */
const stray = () => [...english(words()), ...foreign(words(), put)]

test('the check itself: it finds English where there is some', () => {
  expect(english('Máy chủ 4 · Sort')).toEqual(['sort'])
  expect(english('CPU 23% 4s ago')).toEqual(['secs_ago'])
  expect(english('không có gì: Hostname, sorted, downtime')).toEqual([])
})

test('the check itself: it finds words that are in no table, and lets the table\'s, the data and the keys be', () => {
  expect(foreign('Máy chủ 4 · Thẻ 2 · web-01 deploy@10.0.0.5:2222 · CPU 23% · 12 ms · Ctrl+K · 4 giây trước')).toEqual([])
  expect(foreign('Đã lưu vào /home/me/vssh-vault.backup', ['vssh-vault.backup'])).toEqual([])
  expect(foreign('Máy chủ 4 · ERROR could not be read · Đang nạp')).toEqual(['ERROR', 'could', 'not', 'be', 'read', 'Đang', 'nạp'])
})

test('in Vietnamese no English string is left anywhere: cards in every state, the peek, the dialogs', async () => {
  setLang('vi')
  const user = shell({
    // One host of each kind: well, down, not checked, and still being asked.
    get_host_stats: ({ alias }) => {
      if (alias === 'web-02') throw refused
      if (alias === 'db-primary') return null
      return alias === 'bare' ? new Promise(() => {}) : peek
    },
    ping_host: ({ alias }) => (alias === 'web-02' ? null : 12),
  })
  await vi.waitFor(async () => expect(await card('web-02')).toHaveTextContent('BỊ TỪ CHỐI'))
  expect(await card('web-01')).toHaveTextContent('RAM 4.2/16G')
  expect(await card('db-primary')).toHaveTextContent('chưa kiểm tra')
  expect(await card('bare')).toHaveTextContent('đang hỏi bare…')
  expect(stray()).toEqual([])

  // A session, and the quick peek of a host that answered and of one that is down.
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Kết nối' }))
  await screen.findByRole('tablist', { name: 'Phiên' })
  await says(1, { state: 'up' })
  put.push('top')
  await says(1, { state: 'busy', command: 'top' })
  expect(await tab(/web-01/)).toHaveTextContent('đang chạy top')
  expect(stray()).toEqual([])
  await says(1, { state: 'closed', error: refused })
  await screen.findByRole('button', { name: 'Kết nối lại' })
  expect(stray()).toEqual([])
  // Broadcasting, and a tab that could not connect: a changed host key, a jump host that is down.
  await user.keyboard(`${leader}b`)
  expect(status()).toHaveTextContent('Đang phát: 1')
  expect(stray()).toEqual([])
  for (const [error, name] of [[errors[5], 'KHOÁ MÁY CHỦ: web-01'], [errors[6], 'MÁY TRUNG GIAN: web-01']] as const) {
    await user.click(screen.getByRole('button', { name: 'Kết nối lại' }))
    await says(1, { state: 'closed', error })
    expect(await screen.findByRole('dialog', { name })).toBeInTheDocument()
    expect(stray()).toEqual([])
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Huỷ' }))
  }
  await user.keyboard(`${leader}0`)
  ;(await card('web-02')).focus()
  await user.keyboard(' ')
  expect(await card('web-02')).toHaveAttribute('data-marked')
  expect(stray()).toEqual([])
  await user.keyboard(' ')
  ;(await card('web-01')).focus()
  await user.keyboard('v')
  expect(screen.getByRole('dialog', { name: 'Xem nhanh: web-01' })).toHaveTextContent('Đã chạy')
  expect(stray()).toEqual([])
  ;(await card('web-02')).focus()
  await user.keyboard('v')
  expect(await screen.findByRole('dialog', { name: 'Xem nhanh: web-02' })).toHaveTextContent('thử lại')
  expect(stray()).toEqual([])
  await user.keyboard('{Escape}')

  // A search that finds nothing, the palette, an address typed there.
  await user.keyboard('/zzz')
  expect(screen.getByText('Không có máy chủ nào khớp.')).toBeInTheDocument()
  expect(stray()).toEqual([])
  await user.keyboard('{Escape}{Control>}k{/Control}')
  expect(screen.getByRole('dialog', { name: 'Bảng lệnh' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  put.push('sandbox.example')
  await user.type(palette(), 'sandbox.example')
  expect(screen.getByRole('option', { name: /Kết nối tới sandbox.example/ })).toHaveTextContent('Kết nối nhanh · Không có gì được lưu.')
  expect(stray()).toEqual([])
})

test('with no hosts, in Vietnamese, the empty dashboard is Vietnamese too', async () => {
  setLang('vi')
  shell({ list_hosts: () => [], list_snippets: () => [] })
  expect(await screen.findByRole('heading', { name: 'Chưa có máy chủ nào.' })).toBeInTheDocument()
  expect(stray()).toEqual([])
})

test('every string of the table is one the window uses', async () => {
  const sources = import.meta.glob(['./**/*.svelte', './lib/*.ts', '!./**/*.test.ts'], { query: '?raw', import: 'default', eager: true })
  const code = Object.values(sources).join('\n')
  const unused = Object.keys(strings.en).filter((key) => !code.includes(`t('${key}')`))
  expect(unused).toEqual([])
})

test('f on a card, or its Files button, opens the host\'s files in a tab of their own, once per host', async () => {
  const user = shell()
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  expect(await tab(/Files web-01/)).toHaveAttribute('aria-selected', 'true')
  expect(screen.getByRole('tabpanel', { name: 'Files web-01' })).toBeVisible()
  expect(await screen.findByRole('row', { name: /nginx/ })).toBeInTheDocument()
  expect(asked('sftp_list')).toEqual([{ host: 'web-01', path: '/home/deploy' }])
  // The keys are the file view's. The status bar lists none: the view's own buttons name theirs.
  expect(screen.getByRole('grid', { name: 'local' })).toHaveFocus()
  expect(status().querySelector('kbd')).toBeNull()
  expect(status()).toHaveTextContent('Hosts 4 Snippets 1 Sessions 0')
  const pane = within(screen.getByRole('region', { name: 'local' }))
  expect(pane.getByRole('button', { name: 'Filter' })).toHaveAttribute('title', 'Filter (/, Ctrl+F)')
  expect(pane.getByRole('button', { name: 'Hidden files' })).toHaveAttribute('title', 'Hidden files (.)')
  expect(screen.getByRole('button', { name: 'Upload' })).toHaveAttribute('title', 'Upload (C)')
  // It is no session: nothing was logged in to for a shell.
  expect(asked('session_open')).toHaveLength(0)
  expect(status()).toHaveTextContent('Sessions 0')

  // From the dashboard again, the card's button goes to the tab that is open.
  await user.keyboard(`${leader}0`)
  expect(screen.getByRole('tabpanel', { hidden: true })).not.toBeVisible()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Files' }))
  expect(screen.getAllByRole('tab')).toHaveLength(2)
  expect(asked('sftp_list')).toHaveLength(1)
  await user.keyboard(`${leader}0`)
  await user.click(within(await card('web-02')).getByRole('button', { name: 'Files' }))
  expect(await tab(/2 Files web-02/)).toHaveAttribute('aria-selected', 'true')

  // Ctrl-W closes the one in front, as it closes a terminal; so does its ×.
  await user.keyboard('{Control>}w{/Control}')
  expect(screen.queryByRole('tab', { name: /Files web-02/ })).toBeNull()
  expect(await tab(/Files web-01/)).toHaveAttribute('aria-selected', 'true')
  await user.click(screen.getByRole('button', { name: 'close Files web-01' }))
  expect(screen.queryByRole('tablist')).toBeNull()
  expect(screen.getByRole('region', { name: 'Hosts' })).toBeVisible()
})

test('a file view next to a terminal: its tab comes after the sessions, in Vietnamese too', async () => {
  const user = shell()
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  await user.keyboard(`${leader}0`)
  ;(await card('web-01')).focus()
  await user.keyboard('{Enter}')
  expect(await tab(/1 web-01/)).toHaveAttribute('aria-selected', 'true')
  expect(await tab(/2 Files web-01/)).toHaveAttribute('aria-selected', 'false')
  await user.keyboard(`${leader}2`)
  expect(await tab(/2 Files web-01/)).toHaveAttribute('aria-selected', 'true')
  setLang('vi')
  expect(await tab(/2 Tệp web-01/)).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Tải xuống' })).toHaveAttribute('title', 'Tải xuống (C)')
  expect(screen.getAllByRole('button', { name: 'Tệp ẩn' })[0]).toHaveAttribute('title', 'Tệp ẩn (.)')
  await user.keyboard(`${leader}0`)
  expect(within(await card('web-01')).getByRole('button', { name: 'Tệp' })).toBeInTheDocument()
  ;(await card('web-01')).focus()
  await vi.waitFor(() => expect(status()).toHaveTextContent('web-01 deploy@'))
  expect(within(await card('web-01')).getByRole('button', { name: 'Tệp' })).toHaveAttribute('title', 'Tệp (F)')
})

// --- D5: snippets, batches, forwards -----------------------------------------

test('s on the dashboard, or the palette\'s Snippets, opens the snippets; one added there is counted in the status bar', async () => {
  let list = snippets
  const user = shell({
    list_snippets: () => list,
    snippets_create: (args) => {
      const made = { id: 2, ...args } as (typeof snippets)[0]
      list = [...list, made]
      return made
    },
  })
  await card('web-01')
  await user.keyboard('s')
  const dialog = await screen.findByRole('dialog', { name: 'Snippets' })
  expect(within(dialog).getByRole('listitem')).toHaveTextContent('up all hosts uptime')
  await user.click(within(dialog).getByRole('button', { name: 'Add' }))
  // A letter typed into the form is a letter: no key of the dashboard's.
  await user.type(within(dialog).getByRole('textbox', { name: 'Name' }), 'disks')
  await user.type(within(dialog).getByRole('textbox', { name: 'Command' }), 'df -h /')
  await user.click(within(dialog).getByRole('button', { name: 'Save' }))
  await vi.waitFor(() => expect(status()).toHaveTextContent('Snippets 2'))
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'snip{Enter}')
  expect(await screen.findByRole('dialog', { name: 'Snippets' })).toBeInTheDocument()
  expect(screen.getAllByRole('dialog')).toHaveLength(1)
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // And for the pointer, their count in the status bar.
  await user.click(within(status()).getByRole('button', { name: 'Snippets 2' }))
  expect(await screen.findByRole('dialog', { name: 'Snippets' })).toBeInTheDocument()
  setLang('vi')
  await vi.waitFor(() => expect(status()).toHaveTextContent('Đoạn lệnh 2'))
})

test('a snippet is run on the hosts marked on the dashboard, and each one\'s answer comes in a dialog of its own', async () => {
  const user = shell({ snippet_run: () => 1 })
  ;(await card('db-primary')).focus()
  await user.keyboard(' ')
  ;(await card('web-01')).focus()
  await user.keyboard(' s')
  const dialog = await screen.findByRole('dialog', { name: 'Snippets' })
  await user.click(within(dialog).getByRole('button', { name: 'Run up' }))
  expect(within(dialog).getByRole('checkbox', { name: 'web-01' })).toBeChecked()
  expect(within(dialog).getByRole('checkbox', { name: 'db-primary' })).toBeChecked()
  expect(within(dialog).getByRole('checkbox', { name: 'web-02' })).not.toBeChecked()
  await user.click(within(dialog).getByRole('button', { name: 'Run' }))

  const batch = await screen.findByRole('dialog', { name: 'Run on 2 host(s)' })
  expect(screen.getAllByRole('dialog')).toHaveLength(1)
  await vi.waitFor(() =>
    expect(asked('snippet_run')).toEqual([{ hosts: ['web-01', 'db-primary'], command: 'uptime', params: {} }]),
  )
  await emit('batch-progress', { id: 1, host: 'web-01', output: 'up 3 days\n', error: null })
  await emit('batch-progress', { id: 1, host: 'db-primary', output: null, error: refused })
  await vi.waitFor(() => expect(batch).toHaveTextContent('up 3 days'))
  await vi.waitFor(() => expect(within(batch).getByRole('alert')).toHaveTextContent('✗ REFUSED'))
})

test('with nothing marked, a snippet is for the selected host', async () => {
  const user = shell()
  ;(await card('web-02')).focus()
  await user.keyboard('s')
  const dialog = await screen.findByRole('dialog', { name: 'Snippets' })
  await user.click(within(dialog).getByRole('button', { name: 'Run up' }))
  expect(within(dialog).getByRole('checkbox', { name: 'web-02' })).toBeChecked()
  expect(within(dialog).getByRole('checkbox', { name: 'web-01' })).not.toBeChecked()
})

test('x on the dashboard: a command for the marked hosts, or the selected one; the marks stay for the next', async () => {
  const user = shell({ snippet_run: () => 1 })
  await card('web-01')
  // With no host there is nobody to run on.
  await user.keyboard('x')
  expect(screen.queryByRole('dialog')).toBeNull()

  ;(await card('web-02')).focus()
  await vi.waitFor(() => expect(status()).toHaveTextContent('web-02'))
  await user.keyboard('x')
  const one = await screen.findByRole('dialog', { name: 'Run on 1 host(s)' })
  // An x typed into the line is a letter.
  await user.type(within(one).getByRole('textbox', { name: 'Command' }), 'xargs --version{Enter}')
  await vi.waitFor(() =>
    expect(asked('snippet_run')).toEqual([{ hosts: ['web-02'], command: 'xargs --version', params: {} }]),
  )
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

  ;(await card('db-primary')).focus()
  await user.keyboard(' ')
  ;(await card('web-01')).focus()
  await user.keyboard(' x')
  const two = await screen.findByRole('dialog', { name: 'Run on 2 host(s)' })
  await user.type(within(two).getByRole('textbox', { name: 'Command' }), 'uptime{Enter}')
  await vi.waitFor(() => expect(asked('snippet_run')[1]).toEqual({ hosts: ['web-01', 'db-primary'], command: 'uptime', params: {} }))
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // For the pointer, next to Connect; in Vietnamese, the TUI's words.
  expect(await card('web-01')).toHaveAttribute('data-marked')
  await user.click(screen.getByRole('button', { name: 'Run 2' }))
  expect(await screen.findByRole('dialog', { name: 'Run on 2 host(s)' })).toBeInTheDocument()
  setLang('vi')
  expect(await screen.findByRole('dialog', { name: 'Chạy trên 2 máy' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Chép tất cả', hidden: true })).toBeInTheDocument()
})

test('L on a card, or its Forwards button, opens the host\'s port forwards', async () => {
  const user = shell({ forwards_list: () => forwards, forward_toggle: () => true })
  ;(await card('web-02')).focus()
  await vi.waitFor(() => expect(status()).toHaveTextContent('web-02'))
  // With Shift, as the status bar used to say it, or without.
  await user.keyboard('l')
  expect(await screen.findByRole('dialog', { name: 'Forwards web-02' })).toBeInTheDocument()
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  ;(await card('web-02')).focus()
  await user.keyboard('L')
  const dialog = await screen.findByRole('dialog', { name: 'Forwards web-02' })
  expect(asked('forwards_list')).toEqual([{ host: 'web-02' }, { host: 'web-02' }])
  const web = await within(dialog).findByRole('switch', { name: 'L 8080 localhost:80' })
  await user.click(web)
  expect(asked('forward_toggle')).toEqual([{ host: 'web-02', kind: 'L', value: '8080 localhost:80' }])
  await emit('forward-state', { host: 'web-02', kind: 'L', value: '8080 localhost:80', state: 'on', error: null })
  await vi.waitFor(() => expect(web).toHaveTextContent('● On'))
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // It listens on behind the dialog that is shut: the rail counts it, until the shell says it ended.
  expect(screen.getByRole('button', { name: 'Forwards 1' })).toBeInTheDocument()
  await emit('forward-state', { host: 'web-02', kind: 'L', value: '8080 localhost:80', state: 'off', error: null })
  await vi.waitFor(() => expect(screen.queryByRole('button', { name: 'Forwards 1' })).toBeNull())

  await user.click(within(await card('web-01')).getByRole('button', { name: 'Forwards' }))
  expect(await screen.findByRole('dialog', { name: 'Forwards web-01' })).toBeInTheDocument()
  setLang('vi')
  expect(await screen.findByRole('dialog', { name: 'Chuyển tiếp web-01' })).toBeInTheDocument()
  expect(within(await card('web-01')).getByRole('button', { name: 'Chuyển tiếp' })).toHaveAttribute('title', 'Chuyển tiếp (L)')
})

// --- D6: the vault, the host form, the key setup ----------------------------

/** The sidebar's way to the vault, which says what state it is in. */
const vaultButton = (name: string | RegExp) => within(screen.getByRole('navigation')).findByRole('button', { name })
const shut = async () => {
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
}

test('a locked vault asks for its passphrase as the app opens; unlocked, it stays so and the sidebar says it', async () => {
  let open = false
  const user = shell({
    vault_state: () => (open ? 'unlocked' : 'locked'),
    vault_unlock: () => void (open = true),
    vault_hosts: () => ['web-01'],
  })
  const dialog = await screen.findByRole('dialog', { name: 'Vault' })
  expect(await vaultButton('Vault locked')).toBeInTheDocument()
  await user.type(within(dialog).getByLabelText('Vault passphrase'), 'master{Enter}')
  expect(asked('vault_unlock')).toEqual([{ passphrase: 'master' }])
  // That was all it asked for: the way to the hosts is free.
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(await vaultButton('Vault unlocked')).toBeInTheDocument()

  // The manager, from the sidebar and from the palette.
  await user.click(await vaultButton('Vault unlocked'))
  const manager = await screen.findByRole('dialog', { name: 'Vault' })
  await vi.waitFor(() => expect(manager).toHaveTextContent('Vault unlocked Hosts 4 Passwords 1'))
  await shut()
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'vault{Enter}')
  expect(await screen.findByRole('dialog', { name: 'Vault' })).toBeInTheDocument()
  expect(screen.getAllByRole('dialog')).toHaveLength(1)
  setLang('vi')
  expect(await screen.findByRole('dialog', { name: 'Kho' })).toHaveTextContent('Kho đã mở')
  expect(await vaultButton('Kho đã mở')).toBeInTheDocument()
})

test('a vault left locked asks no more; without one nothing is asked at all', async () => {
  const user = shell({ vault_state: () => 'locked' })
  await user.click(within(await screen.findByRole('dialog', { name: 'Vault' })).getByRole('button', { name: 'Cancel' }))
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(await vaultButton('Vault locked')).toBeInTheDocument()
  expect(asked('vault_unlock')).toEqual([])
  cleanup()

  shell()
  await card('web-01')
  expect(asked('vault_state')).toHaveLength(2)
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(await vaultButton('Vault')).toBeInTheDocument()
})

/** The shell over hosts that can be added to and taken from. */
function editable(over: Record<string, (args: Args) => unknown> = {}) {
  let list: Host[] = hosts
  const made = (form: { alias: string; hostname: string }): Host => ({ ...hosts[3], alias: form.alias, hostname: form.hostname })
  return shell({
    list_hosts: () => list,
    hosts_form: ({ alias }) => {
      const host = list.find((host) => host.alias === alias)!
      const { grp, tags, notes, pinned } = host.meta
      return { alias, hostname: host.hostname ?? '', user: host.user ?? '', port: host.port, identity_file: '', proxy_jump: '', grp, tags, notes, pinned }
    },
    hosts_add: ({ form }) => {
      const host = made(form as Parameters<typeof made>[0])
      list = [...list, host]
      return host
    },
    hosts_update: ({ alias, form }) => {
      const host = made(form as Parameters<typeof made>[0])
      list = list.map((old) => (old.alias === alias ? host : old))
      return host
    },
    hosts_delete: ({ alias }) => void (list = list.filter((host) => host.alias !== alias)),
    ...over,
  })
}

test('a on the dashboard, its Add host button or the palette opens the host form; the host saved is there and selected', async () => {
  const user = editable()
  await card('web-01')
  await user.keyboard('a')
  const dialog = await screen.findByRole('dialog', { name: 'Add host' })
  // Letters typed into the form are letters: no key of the dashboard's.
  await user.type(within(dialog).getByLabelText('Alias'), 'sandbox')
  await user.type(within(dialog).getByLabelText('Host'), 'sandbox.example')
  await user.click(within(dialog).getByRole('button', { name: 'Save' }))
  expect(asked('hosts_add')[0].form).toMatchObject({ alias: 'sandbox', hostname: 'sandbox.example' })
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // Not at the next round of the dashboard: at once.
  expect(await card('sandbox')).toHaveAttribute('aria-current', 'true')
  await vi.waitFor(async () => expect(await card('sandbox')).toHaveFocus())
  expect(status()).toHaveTextContent('Hosts 5')

  await user.click(screen.getByRole('button', { name: 'Add host' }))
  expect(await screen.findByRole('dialog', { name: 'Add host' })).toBeInTheDocument()
  await shut()
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'add host{Enter}')
  expect(await screen.findByRole('dialog', { name: 'Add host' })).toBeInTheDocument()
  expect(screen.getAllByRole('dialog')).toHaveLength(1)
})

test('hosts taken over from another ssh config are on the dashboard at once', async () => {
  const user = editable({ hosts_import_preview: () => [hosts[0]], hosts_import_ssh_config: () => ['web-01'] })
  await card('web-01')
  await user.keyboard('a')
  await user.click(await screen.findByRole('button', { name: 'Import from ssh config' }))
  await user.click(await screen.findByRole('button', { name: 'Import 1' }))
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(asked('list_hosts')).toHaveLength(2)
  expect(await card('web-01')).toHaveAttribute('aria-current', 'true')
})

test('with no hosts yet the empty dashboard has the way to the first one', async () => {
  const user = shell({ list_hosts: () => [] })
  await user.click(await screen.findByRole('button', { name: 'Add host' }))
  expect(await screen.findByRole('dialog', { name: 'Add host' })).toBeInTheDocument()
})

test('e on a card, or its Edit button, opens the form on that host; renamed, it is still the selected one', async () => {
  const user = editable()
  ;(await card('web-02')).focus()
  await vi.waitFor(() => expect(status()).toHaveTextContent('web-02'))
  await user.keyboard('e')
  const dialog = await screen.findByRole('dialog', { name: 'Edit host' })
  await vi.waitFor(() => expect(within(dialog).getByLabelText('Host')).toHaveValue('10.0.0.6'))
  expect(asked('hosts_form')).toEqual([{ alias: 'web-02' }])
  await user.clear(within(dialog).getByLabelText('Alias'))
  await user.type(within(dialog).getByLabelText('Alias'), 'web-two')
  await user.click(within(dialog).getByRole('button', { name: 'Save' }))
  expect(asked('hosts_update')[0]).toMatchObject({ alias: 'web-02', form: { alias: 'web-two', hostname: '10.0.0.6' } })
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(await card('web-two')).toHaveAttribute('aria-current', 'true')
  expect(screen.queryByRole('article', { name: 'web-02' })).toBeNull()

  await user.click(within(await card('web-01')).getByRole('button', { name: 'Edit' }))
  await vi.waitFor(() => expect(within(screen.getByRole('dialog', { name: 'Edit host' })).getByLabelText('Alias')).toHaveValue('web-01'))
})

test('d on a card asks first; Delete then takes the host, and with it its sessions and its files', async () => {
  const user = editable()
  ;(await card('web-02')).focus()
  await user.keyboard('{Enter}')
  await user.keyboard(`${leader}0`)
  ;(await card('web-01')).focus()
  await user.keyboard('{Enter}')
  await user.keyboard(`${leader}0`)
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  await user.keyboard(`${leader}0`)
  expect(screen.getAllByRole('tab')).toHaveLength(4)

  ;(await card('web-01')).focus()
  await user.keyboard('d')
  const dialog = await screen.findByRole('dialog', { name: 'Delete web-01?' })
  expect(dialog).toHaveTextContent('Its open sessions are closed.')
  // Enter is not a yes: the question opens on Cancel.
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(asked('hosts_delete')).toEqual([])
  expect(screen.getAllByRole('tab')).toHaveLength(4)

  ;(await card('web-01')).focus()
  await user.keyboard('{Delete}')
  await user.click(within(await screen.findByRole('dialog', { name: 'Delete web-01?' })).getByRole('button', { name: 'Delete' }))
  expect(asked('hosts_delete')).toEqual([{ alias: 'web-01' }])
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // Its session and its file view are closed; web-02's session is not.
  await vi.waitFor(() => expect(screen.getAllByRole('tab')).toHaveLength(2))
  expect(asked('session_close')).toEqual([{ id: 2 }])
  expect(await tab(/web-02/)).toBeInTheDocument()
  expect(screen.queryByRole('article', { name: 'web-01' })).toBeNull()
  expect(status()).toHaveTextContent('Hosts 3')
})

test('the form\'s own Delete asks the same question; a refusal is said there and nothing is closed', async () => {
  const user = editable({
    hosts_delete: () => {
      throw refused
    },
  })
  ;(await card('web-02')).focus()
  await user.keyboard('e')
  await user.click(await screen.findByRole('button', { name: 'Delete' }))
  const dialog = await screen.findByRole('dialog', { name: 'Delete web-02?' })
  expect(screen.getAllByRole('dialog')).toHaveLength(1)
  await user.click(within(dialog).getByRole('button', { name: 'Delete' }))
  expect(await within(dialog).findByRole('alert')).toHaveTextContent('✗ REFUSED')
  expect(await card('web-02')).toBeInTheDocument()
  setLang('vi')
  expect(await screen.findByRole('dialog', { name: 'Xoá web-02?' })).toHaveTextContent('Các phiên đang mở của máy sẽ đóng.')
})

test('a quick connect in front can be kept: Save this host opens the form with what was typed for it', async () => {
  const user = editable()
  ;(await card('web-01')).focus()
  await user.keyboard('{Enter}')
  // A session on a host that is saved has nothing to save.
  await tab(/web-01/)
  expect(screen.queryByRole('button', { name: 'Save this host' })).toBeNull()

  await user.keyboard(ctrlK)
  await user.type(palette(), 'root@sandbox.example:2200{Enter}')
  await tab(/root@sandbox.example:2200/)

  await user.click(within(status()).getByRole('button', { name: 'Save this host' }))
  const dialog = await screen.findByRole('dialog', { name: 'Add host' })
  expect(within(dialog).getByLabelText('Alias')).toHaveValue('sandbox.example')
  expect(within(dialog).getByLabelText('Host')).toHaveValue('sandbox.example')
  expect(within(dialog).getByLabelText('User')).toHaveValue('root')
  expect(within(dialog).getByLabelText('Port')).toHaveValue('2200')
  await user.click(within(dialog).getByRole('button', { name: 'Save' }))
  expect(asked('hosts_add')[0].form).toMatchObject({ alias: 'sandbox.example', hostname: 'sandbox.example', user: 'root', port: 2200 })
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // It is a host now: the terminal stays in front, with nothing left to save.
  expect(await tab(/root@sandbox.example:2200/)).toHaveAttribute('aria-selected', 'true')
  expect(screen.queryByRole('button', { name: 'Save this host' })).toBeNull()
  expect(status()).toHaveTextContent('Hosts 5')
})

test('a host signed in to with a vault password is given a key from its form; afterwards the form says Key', async () => {
  let key = ''
  const user = editable({
    vault_state: () => 'unlocked',
    vault_hosts: () => ['web-02'],
    hosts_form: ({ alias }) => ({ alias, hostname: '10.0.0.6', user: 'deploy', port: null, identity_file: key, proxy_jump: '', grp: '', tags: '', notes: '', pinned: false }),
    ssh_key_setup: () => (key = '/home/me/.ssh/vssh_web-02_ed25519'),
  })
  ;(await card('web-02')).focus()
  await user.keyboard('e')
  await vi.waitFor(() => expect(screen.getByRole('radio', { name: 'Vault password' })).toBeChecked())
  await user.click(screen.getByRole('button', { name: 'Set up SSH key' }))
  const wizard = await screen.findByRole('dialog', { name: 'Set up SSH key web-02' })
  expect(screen.getAllByRole('dialog')).toHaveLength(1)
  const before = asked('list_hosts').length
  await user.click(within(wizard).getByRole('button', { name: 'Start' }))
  expect(asked('ssh_key_setup')).toEqual([{ host: 'web-02' }])
  expect(await within(wizard).findByRole('status')).toHaveTextContent('web-02 is signed in to with its key from now on.')
  await user.click(within(wizard).getByRole('button', { name: 'close' }))
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // The config was written by the shell: it is read again.
  await vi.waitFor(() => expect(asked('list_hosts').length).toBe(before + 1))

  ;(await card('web-02')).focus()
  await user.keyboard('e')
  await vi.waitFor(() => expect(screen.getByRole('radio', { name: 'Key' })).toBeChecked())
  expect(screen.getByLabelText('Key file')).toHaveValue('/home/me/.ssh/vssh_web-02_ed25519')
  expect(screen.queryByRole('button', { name: 'Set up SSH key' })).toBeNull()
})

test('in Vietnamese no English string is left in the vault, the host form, the import, the question and the key setup', async () => {
  setLang('vi')
  let open = false
  let stop = (_: object) => {}
  const user = editable({
    vault_state: () => (open ? 'unlocked' : 'locked'),
    vault_unlock: ({ passphrase }) => {
      if (passphrase !== 'master') throw errors[7]
      open = true
    },
    vault_hosts: () => ['web-02'],
    vault_export: () => null,
    hosts_import_preview: () => [hosts[3]],
    ssh_key_setup: () => new Promise((_, reject) => (stop = reject)),
  })
  const said = async (name: string) => {
    const dialog = await screen.findByRole('dialog', { name })
    expect(stray()).toEqual([])
    return within(dialog)
  }

  // The vault: locked, wrong three times, unlocked, a backup written.
  put.push('vssh-vault.backup')
  const vault = await said('Kho')
  for (let i = 0; i < 3; i++) await user.type(vault.getByLabelText('Mật khẩu kho'), 'x{Enter}')
  expect(await vault.findByRole('status')).toHaveTextContent('Đã sai 3 lần.')
  expect(stray()).toEqual([])
  await user.type(vault.getByLabelText('Mật khẩu kho'), 'master{Enter}')
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  await user.click(await vaultButton('Kho đã mở'))
  const manager = await said('Kho')
  await user.click(await manager.findByRole('button', { name: 'Xuất bản sao lưu' }))
  expect(await manager.findByRole('status')).toHaveTextContent('Đã lưu vào /home/me/vssh-vault.backup')
  expect(stray()).toEqual([])
  await shut()

  // A new host with everything wrong, then what another config has.
  await card('web-01')
  await user.keyboard('a')
  const add = await said('Thêm máy chủ')
  await user.type(add.getByLabelText('Cổng'), 'x')
  await user.click(add.getByRole('radio', { name: 'Khoá' }))
  await user.click(add.getByRole('button', { name: 'Lưu' }))
  expect(add.getAllByRole('alert')).toHaveLength(3)
  expect(stray()).toEqual([])
  await user.click(add.getByRole('button', { name: 'Nhập từ ssh config' }))
  expect(await screen.findByRole('button', { name: 'Nhập 1' })).toBeInTheDocument()
  await said('Nhập từ ssh config')
  await shut()

  // A host with a password in the vault: its form, its key setup under way, the question before it goes.
  ;(await card('web-02')).focus()
  await user.keyboard('e')
  const edit = await said('Sửa máy chủ')
  await vi.waitFor(() => expect(edit.getByLabelText('Mật khẩu')).toHaveAttribute('placeholder', 'đã lưu mật khẩu'))
  expect(stray()).toEqual([])
  await user.click(edit.getByRole('button', { name: 'Cài khoá SSH' }))
  const keys = await said('Cài khoá SSH web-02')
  await user.click(keys.getByRole('button', { name: 'Bắt đầu' }))
  await emit('key-setup-step', { host: 'web-02', step: 'generate', state: 'running' })
  expect(await keys.findByRole('progressbar', { name: 'đang chạy' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  // While it runs it stays; once it has failed it can be shut.
  screen.getByRole<HTMLDialogElement>('dialog').close()
  expect(screen.getByRole('dialog', { name: 'Cài khoá SSH web-02' })).toBeInTheDocument()
  stop(errors[4])
  expect(await keys.findByRole('alert')).toHaveTextContent('XÁC THỰC')
  expect(stray()).toEqual([])
  await shut()
  ;(await card('web-02')).focus()
  await user.keyboard('d')
  await said('Xoá web-02?')
})

// --- D7: streamer mode --------------------------------------------------------

test('streamer mode: every address in the window is a made-up one, and nothing but what is shown changes', async () => {
  const user = editable({
    get_host_stats: ({ alias }) => {
      if (alias === 'web-02') throw refused
      return peek
    },
    forwards_list: () => [...forwards, { ...forwards[0], host: 'db.internal.example.com', hport: 5432, port: 15432, value: '15432 db.internal.example.com:5432' }],
    forward_toggle: () => true,
    quick_connect: () => 7,
  })
  const real = /10\.0\.0\.\d|db\.internal/
  const [one, two] = ['web-01', 'web-02'].map(fake)
  await vi.waitFor(async () => expect(await card('web-02')).toHaveTextContent('REFUSED Connection refused by 10.0.0.5:2222'))
  expect(words()).toMatch(real)

  await setting(user, /Streamer mode/)
  expect(localStorage.getItem('vssh.streamer_mode')).toBe('true')

  // The cards, a failure's words on one, the sidebar, the status bar.
  expect(await card('web-01')).toHaveTextContent(`deploy@${one}:2222`)
  await vi.waitFor(async () => expect(await card('web-02')).toHaveTextContent(`REFUSED Connection refused by ${one}:2222`))
  expect(await card('web-02')).toHaveTextContent(`deploy@${two}`)
  expect(within(screen.getByRole('navigation')).getByRole('button', { name: /db-primary/ })).toHaveAttribute('title', `postgres@${fake('db-primary')}`)
  ;(await card('web-01')).focus()
  await vi.waitFor(() => expect(status()).toHaveTextContent(`web-01 deploy@${one}:2222`))
  expect(words()).not.toMatch(real)

  // The quick peek, the palette, a host's forwards.
  await user.keyboard('v')
  expect(screen.getByRole('dialog', { name: 'Quick peek: web-01' })).toHaveTextContent(`Host ${one}System`)
  await user.keyboard('{Escape}{Control>}k{/Control}')
  expect(screen.getByRole('option', { name: /web-01/ })).toHaveTextContent(`deploy@${one}:2222`)
  expect(words()).not.toMatch(real)
  await shut()
  ;(await card('web-02')).focus()
  await user.keyboard('L')
  const tunnels = await screen.findByRole('dialog', { name: 'Forwards web-02' })
  // This machine is nobody's secret; where a forward leads on the far side is.
  expect(await within(tunnels).findByRole('switch', { name: 'L 8080 localhost:80' })).toBeInTheDocument()
  expect(within(tunnels).getByRole('switch', { name: `L 15432 ${fake('db.internal.example.com')}:5432` })).toBeInTheDocument()
  expect(words()).not.toMatch(real)
  await user.click(within(tunnels).getByRole('switch', { name: /15432/ }))
  expect(asked('forward_toggle')).toEqual([{ host: 'web-02', kind: 'L', value: '15432 db.internal.example.com:5432' }])
  await shut()

  // The host form: the host is there to be changed, and cannot be read.
  ;(await card('web-01')).focus()
  await user.keyboard('e')
  const form = await screen.findByRole('dialog', { name: 'Edit host' })
  await vi.waitFor(() => expect(within(form).getByLabelText('Host')).toHaveValue('10.0.0.5'))
  expect(within(form).getByLabelText('Host')).toHaveAttribute('type', 'password')
  await user.click(within(form).getByRole('button', { name: 'Save' }))
  expect(asked('hosts_update')[0].form).toMatchObject({ alias: 'web-01', hostname: '10.0.0.5' })
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

  // A quick connect: typed unseen, sent as typed, and its tab is named for an address that is nobody's.
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'deploy@10.0.0.9')
  expect(palette()).toHaveAttribute('data-masked')
  expect(words()).not.toMatch(real)
  await user.click(screen.getByRole('option', { name: /Connect to deploy@/ }))
  expect(asked('quick_connect')).toEqual([{ host: 'deploy@10.0.0.9' }])
  expect(await tab(new RegExp(`deploy@${fake('10.0.0.9').replaceAll('.', '\\.')}`))).toHaveAttribute('aria-selected', 'true')
  expect(screen.getByRole('tabpanel', { name: `deploy@${fake('10.0.0.9')}` })).toBeInTheDocument()
  expect(words()).not.toMatch(real)
  // A host's own tab is named for the host, which is no address.
  await user.keyboard(`${leader}0`)
  ;(await card('web-01')).focus()
  await user.keyboard('{Enter}')
  expect(await tab(/web-01/)).toHaveAttribute('aria-selected', 'true')
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }])

  // Off again: the real ones, everywhere.
  await setting(user, /Streamer mode/)
  expect(localStorage.getItem('vssh.streamer_mode')).toBe('false')
  expect(await tab(/deploy@10\.0\.0\.9/)).toBeInTheDocument()
  await user.keyboard(`${leader}0`)
  // (The fixture's shell saved the form without its user.)
  expect(await card('web-01')).toHaveTextContent('10.0.0.5')
  expect(await card('web-01')).not.toHaveTextContent(one)
  await vi.waitFor(async () => expect(await card('web-02')).toHaveTextContent('REFUSED Connection refused by 10.0.0.5:2222'))
})

test('streamer mode that was left on is on when the app opens, in Vietnamese too', async () => {
  localStorage.setItem('vssh.streamer_mode', 'true')
  vi.resetModules()
  const { streamer, fake } = await import('./lib/streamer.svelte')
  expect(streamer.on).toBe(true)
  setStreamer(true)
  setLang('vi')
  const user = shell()
  expect(await card('web-01')).toHaveTextContent(`deploy@${fake('web-01')}:2222`)
  await user.click(screen.getByRole('button', { name: 'Cài đặt' }))
  expect(screen.getByRole('menuitem', { name: /Chế độ phát trực tiếp/ })).toHaveTextContent('✓')
  expect(stray()).toEqual([])
})

// --- D7: the Day theme --------------------------------------------------------

test('the theme in the settings: Night until Day is chosen, a terminal dark in both', async () => {
  const user = shell()
  expect(document.documentElement.dataset.theme).toBe('night')
  await setting(user, /Theme/)
  expect(document.documentElement.dataset.theme).toBe('day')
  expect(localStorage.getItem('vssh.theme')).toBe('day')

  ;(await card('web-01')).focus()
  await user.keyboard('{Enter}')
  await tab(/web-01/)
  expect(terminals[0].options.theme).toMatchObject({ background: '#1a1b26', foreground: '#c0caf5' })
  // Its ground is the terminal's own, not the window's; the border is the theme's.
  expect(screen.getByRole('tabpanel', { name: 'web-01' })).toHaveClass('bg-term', 'day:border-edge')

  setLang('vi')
  await user.click(await screen.findByRole('button', { name: 'Cài đặt' }))
  const row = screen.getByRole('menuitem', { name: /Giao diện/ })
  expect(row).toHaveTextContent('Giao diện Sáng')
  expect(stray()).toEqual([])
  await user.click(row)
  expect(document.documentElement.dataset.theme).toBe('night')
  expect(stray()).toEqual([])
})

// --- D7: the sweep, over the views the two above do not open -----------------

test('in Vietnamese no word is left untranslated in the files, the snippets, a batch and the forwards, failures and all', async () => {
  setLang('vi')
  const user = shell({
    // web-02 lets nobody in: its files, its forwards and its part of a batch all fail.
    sftp_realpath: ({ host }) => {
      if (host === 'web-02') throw refused
      return '/home/deploy'
    },
    snippet_run: () => 1,
    forwards_list: () => forwards,
    forward_toggle: () => true,
    forward_add: () => {
      throw errors[8]
    },
  })

  // The files of a host that answers, and of one that does not.
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  expect(await tab(/Tệp web-01/)).toHaveAttribute('aria-selected', 'true')
  await vi.waitFor(() => expect(asked('sftp_list')).toHaveLength(1))
  await screen.findAllByText(listing[0].name)
  expect(stray()).toEqual([])
  await user.keyboard(`${leader}0`)
  ;(await card('web-02')).focus()
  await user.keyboard('f')
  expect(await tab(/Tệp web-02/)).toHaveAttribute('aria-selected', 'true')
  await vi.waitFor(() => expect(screen.getAllByRole('alert').at(-1)).toHaveTextContent('BỊ TỪ CHỐI'))
  expect(stray()).toEqual([])
  await user.keyboard(`${leader}0`)

  // The snippets: the list, the form for a new one, the form that runs one.
  ;(await card('web-02')).focus()
  await user.keyboard(' ')
  ;(await card('web-01')).focus()
  await user.keyboard(' s')
  const list = await screen.findByRole('dialog', { name: 'Đoạn lệnh' })
  expect(stray()).toEqual([])
  await user.click(within(list).getByRole('button', { name: 'Thêm' }))
  expect(stray()).toEqual([])
  await user.click(within(list).getByRole('button', { name: 'Huỷ' }))
  await user.click(within(list).getByRole('button', { name: 'Chạy up' }))
  expect(stray()).toEqual([])
  await user.click(within(list).getByRole('button', { name: 'Chạy' }))

  // The batch: one host answered, one said nothing, one failed.
  const batch = await screen.findByRole('dialog', { name: 'Chạy trên 2 máy' })
  expect(stray()).toEqual([])
  put.push('up 3 days')
  await emit('batch-progress', { id: 1, host: 'web-01', output: 'up 3 days\n', error: null })
  await emit('batch-progress', { id: 1, host: 'web-02', output: null, error: refused })
  await vi.waitFor(() => expect(within(batch).getByRole('alert')).toHaveTextContent('✗ BỊ TỪ CHỐI'))
  expect(stray()).toEqual([])
  await shut()
  await user.keyboard('x')
  const typed = await screen.findByRole('dialog', { name: 'Chạy trên 2 máy' })
  put.push('true')
  await user.type(within(typed).getByRole('textbox', { name: 'Lệnh' }), 'true{Enter}')
  await emit('batch-progress', { id: 1, host: 'web-01', output: '', error: null })
  await vi.waitFor(() => expect(typed).toHaveTextContent('(không có kết quả)'))
  expect(stray()).toEqual([])
  await shut()

  // The forwards: the list, one that does not start, one that cannot be added.
  ;(await card('web-01')).focus()
  await user.keyboard('L')
  const tunnels = await screen.findByRole('dialog', { name: 'Chuyển tiếp web-01' })
  const first = await within(tunnels).findByRole('switch', { name: 'L 8080 localhost:80' })
  expect(stray()).toEqual([])
  await user.click(first)
  await emit('forward-state', { host: 'web-01', kind: 'L', value: '8080 localhost:80', state: 'off', error: errors[8] })
  await vi.waitFor(() => expect(within(tunnels).getByRole('alert')).toHaveTextContent('CỔNG ĐANG BẬN'))
  expect(stray()).toEqual([])
  await user.type(within(tunnels).getByLabelText('Cổng cục bộ'), '8080')
  await user.type(within(tunnels).getByLabelText('Đích'), 'localhost:80')
  await user.click(within(tunnels).getByRole('button', { name: 'Thêm' }))
  await vi.waitFor(() => expect(asked('forward_add')).toHaveLength(1))
  expect(stray()).toEqual([])
})

test('a command the shell cannot even be asked: the failure is labelled in the window\'s language', async () => {
  setLang('vi')
  // No such command in this shell: Tauri's own refusal, which is no failure of the core's.
  const user = shell({ resolve_conn_error: () => Promise.reject('not a failure the core knows') })
  ;(await card('web-01')).focus()
  await user.keyboard('L')
  const alert = await within(await screen.findByRole('dialog', { name: 'Chuyển tiếp web-01' })).findByRole('alert')
  expect(alert).toHaveTextContent('✗ LỖI')
  expect(alert).not.toHaveTextContent('ERROR')
})

test('hosts that cannot be read, in Vietnamese: the reason and nothing in English around it', async () => {
  setLang('vi')
  shell({ list_hosts: fails })
  expect(await screen.findByRole('alert')).toHaveTextContent('BỊ TỪ CHỐI')
  expect(stray()).toEqual([])
})

// --- D9: the rail and the top bar ---------------------------------------------

/** The rail is the first navigation of the window; a file pane's path is one too. */
const rail = (name: string | RegExp) => within(screen.getAllByRole('navigation')[0]).getByRole('button', { name })
/** The top bar: the header of the page, which a card's own header is not. */
const head = () => screen.getByRole('heading', { level: 1 }).closest('header')!

test('the rail goes where the keys go: a session, the dashboard and its search, the selected host\'s files and forwards, the snippets', async () => {
  const user = shell({ forwards_list: () => forwards })
  // Nothing is selected and nothing is open: there is nowhere for these to go.
  await card('web-01')
  for (const name of ['Sessions', 'Files', 'Forwards']) expect(rail(name), name).toBeDisabled()
  expect(rail('Status')).toHaveAttribute('aria-current', 'page')
  expect(screen.getByRole('heading', { level: 1, name: 'Status' })).toBeInTheDocument()

  ;(await card('web-02')).focus()
  await user.keyboard('{Enter}')
  await tab(/web-02/)
  // The terminal is in front: its tab is its head, with no band of a title over it, and the rail says where one is.
  expect(screen.queryByRole('heading', { level: 1 })).toBeNull()
  expect(document.querySelector('[data-framed]')).toBeNull()
  expect(rail('Sessions 1')).toHaveAttribute('aria-current', 'page')
  await user.click(rail('Status'))
  expect(await card('web-02')).toBeVisible()
  expect(rail('Status')).toHaveAttribute('aria-current', 'page')
  await user.click(rail('Sessions 1'))
  expect(await tab(/web-02/)).toHaveAttribute('aria-selected', 'true')
  expect(screen.queryByRole('article')).not.toBeInTheDocument()

  // Hosts: the dashboard, at its search.
  await user.click(rail('Hosts 4'))
  await vi.waitFor(() => expect(screen.getByRole('searchbox', { name: 'Search hosts, #tag' })).toHaveFocus())

  await user.click(rail('Files'))
  expect(await tab(/Files web-02/)).toHaveAttribute('aria-selected', 'true')
  expect(screen.queryByRole('heading', { level: 1 })).toBeNull()
  expect(rail('Files')).toHaveAttribute('aria-current', 'page')
  expect(asked('sftp_realpath')).toEqual([{ host: 'web-02', path: '' }])

  await user.click(rail('Forwards'))
  expect(await screen.findByRole('dialog', { name: 'Forwards web-02' })).toBeInTheDocument()
  await shut()
  await user.click(rail(/Snippets/))
  expect(await screen.findByRole('dialog', { name: 'Snippets' })).toBeInTheDocument()
})

test('the rail\'s search opens the palette, and the line under Status counts who answers', async () => {
  const user = shell({
    get_host_stats: ({ alias }) => {
      if (alias === 'web-02') throw refused
      return peek
    },
  })
  await vi.waitFor(() => expect(head()).toHaveTextContent('4 Hosts · 3 reachable · 1 down'))
  // One search a view: the head has none, the dashboard's own field filters its cards.
  expect(within(head()).queryByRole('button')).toBeNull()
  expect(screen.getAllByRole('button', { name: 'Command palette' })).toHaveLength(1)
  await user.click(rail('Command palette'))
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
})

test('the rail and the top bar in Vietnamese: no word of theirs is left in English', async () => {
  setLang('vi')
  shell()
  await card('web-01')
  for (const name of ['Trạng thái', 'Máy chủ 4', 'Phiên', 'Tệp', /Đoạn lệnh/, 'Chuyển tiếp', 'Kho']) expect(rail(name)).toBeInTheDocument()
  await vi.waitFor(() => expect(head()).toHaveTextContent('4 Máy chủ · 4 kết nối được · 0 mất kết nối'))
  expect(stray()).toEqual([])
})

// --- D10: the keys of a desktop app, and where a person finds them ------------

const sheetOpen = () => screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })

test('the cheatsheet opens five ways: ?, F1, Ctrl+/, the settings, the palette; the keys of the view in front first', async () => {
  const user = shell()
  await card('web-01')
  expect(sheetOpen()).toBeNull()
  for (const keys of ['?', '{F1}', '{Control>}/{/Control}']) {
    await user.keyboard(keys)
    expect(sheetOpen(), keys).toBeInTheDocument()
    await shut()
  }
  await setting(user, /Keyboard shortcuts/)
  // On the dashboard its keys come first; the rest follow.
  expect(within(sheetOpen()!).getAllByRole('region').map((group) => group.getAttribute('aria-label'))).toEqual([
    'Hosts',
    'Anywhere',
    'In a session, after Ctrl+A',
    'Files',
    'With the mouse',
    'Command results',
  ])
  // Under a dialog the keys are the dialog's: a ? typed in its field is a character.
  await user.type(within(sheetOpen()!).getByRole('textbox'), 'ctrl?')
  expect(within(sheetOpen()!).getByRole('textbox')).toHaveValue('ctrl?')
  await shut()
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'shortcuts{Enter}')
  expect(sheetOpen()).toBeInTheDocument()
  await shut()
  // In a field ? is a character too; F1 is not.
  await user.type(screen.getByRole('searchbox'), '?')
  expect(sheetOpen()).toBeNull()
  expect(screen.getByRole('searchbox')).toHaveValue('?')
  await user.keyboard('{F1}')
  expect(sheetOpen()).toBeInTheDocument()
})

test('with a terminal in front the window takes no key a shell uses: ?, F1, Ctrl+/, Ctrl+T, Ctrl+F stay the shell\'s; Ctrl-A ? is the cheatsheet', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/web-01/)
  for (const keys of ['?', '{F1}', '{Control>}/{/Control}', '{Control>}t{/Control}', '{Control>}f{/Control}']) {
    await user.keyboard(keys)
    expect(screen.queryByRole('dialog'), keys).toBeNull()
  }
  expect(screen.getByRole('searchbox', { hidden: true })).not.toHaveFocus()
  await user.keyboard(`${leader}?`)
  expect(sheetOpen()).toBeInTheDocument()
  // The session's keys first.
  expect(within(sheetOpen()!).getAllByRole('region')[0]).toHaveAccessibleName('In a session, after Ctrl+A')
  await shut()
  // The settings at the rail's foot are there in every view.
  await setting(user, /Keyboard shortcuts/)
  expect(sheetOpen()).toBeInTheDocument()
})

test('Ctrl+Tab and Ctrl+Shift+Tab go through the tabs, from a terminal too; Ctrl-A q and d are the leader\'s for the hosts and for closing', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/web-01/)
  await user.keyboard(`${leader}0`)
  await user.click(within(await card('web-02')).getByRole('button', { name: 'Connect' }))
  const picked = () => screen.getByRole('tab', { selected: true })
  expect(picked()).toHaveTextContent('2 web-02')
  const next = '{Control>}{Tab}{/Control}'
  const back = '{Control>}{Shift>}{Tab}{/Shift}{/Control}'
  await user.keyboard(back)
  expect(picked()).toHaveTextContent('1 web-01')
  expect(terminals[0].focused).toBe(true)
  await user.keyboard(next)
  expect(picked()).toHaveTextContent('2 web-02')
  await user.keyboard(next)
  expect(picked()).toHaveTextContent('0 Hosts')
  await user.keyboard(back)
  expect(picked()).toHaveTextContent('2 web-02')
  // As WebKitGTK says Shift+Tab: a key with no name (ISO_Left_Tab), in Tab's place. After the leader too.
  await fireEvent.keyDown(document.body, { key: 'Unidentified', code: 'Tab', ctrlKey: true, shiftKey: true })
  expect(picked()).toHaveTextContent('1 web-01')
  await user.keyboard(leader)
  await fireEvent.keyDown(document.body, { key: 'Unidentified', code: 'Tab', shiftKey: true })
  expect(picked()).toHaveTextContent('0 Hosts')
  await user.keyboard(back)
  expect(picked()).toHaveTextContent('2 web-02')
  // Nothing of it reached a shell.
  expect(asked('session_write')).toEqual([])
  await user.keyboard(`${leader}q`)
  expect(picked()).toHaveTextContent('0 Hosts')
  await user.keyboard(`${back}${leader}d`)
  expect(asked('session_close')).toEqual([{ id: 2 }])
})

test('Ctrl+T is a new tab: the palette, where a host chosen is connected to and an address is too; Ctrl+F is the search, on the dashboard and in a file pane', async () => {
  const user = shell()
  await card('web-01')
  await user.keyboard('{Control>}t{/Control}')
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
  // Asked for a tab, a host is connected to, not shown on the dashboard.
  await user.type(palette(), 'db{Enter}')
  expect(await tab(/1 db-primary/)).toHaveAttribute('aria-selected', 'true')
  expect(asked('session_open')).toEqual([{ alias: 'db-primary' }])
  await user.keyboard(`${leader}0{Control>}t{/Control}`)
  await user.type(palette(), '10.0.0.9{Enter}')
  expect(await tab(/10.0.0.9/)).toHaveAttribute('aria-selected', 'true')
  expect(asked('quick_connect')).toEqual([{ host: '10.0.0.9' }])
  await user.keyboard(`${leader}0`)
  const search = screen.getByRole('searchbox', { name: 'Search hosts, #tag' })
  // No slash in the field: its tooltip names the keys.
  expect(search).toHaveAttribute('placeholder', 'Search hosts, #tag')
  expect(search).toHaveAttribute('title', 'Search hosts, #tag (/, Ctrl+F)')
  expect(search).not.toHaveFocus()
  await user.keyboard('{Control>}f{/Control}')
  expect(search).toHaveFocus()
  // From the field too: it stays there, and no f is typed.
  await user.keyboard('{Control>}f{/Control}')
  expect(search).toHaveFocus()
  expect(search).toHaveValue('')
  await user.keyboard('{Escape}')
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  await screen.findByRole('row', { name: /nginx/ })
  expect(screen.getByRole('grid', { name: 'local' })).toHaveFocus()
  await user.keyboard('{Control>}f{/Control}')
  expect(within(screen.getByRole('region', { name: 'local' })).getByRole('searchbox', { name: 'Filter' })).toHaveFocus()
  // The dashboard's search is behind the tab: it was not asked.
  expect(search).not.toHaveFocus()
})

test('the cheatsheet in Vietnamese: every group and row, with nothing in English and no raw row of keys left in the window', async () => {
  setLang('vi')
  const user = shell()
  await card('web-01')
  await user.keyboard('?')
  const dialog = screen.getByRole('dialog', { name: 'Phím tắt' })
  expect(within(dialog).getAllByRole('region').map((group) => group.getAttribute('aria-label'))).toEqual([
    'Máy chủ',
    'Mọi nơi',
    'Trong phiên, sau Ctrl+A',
    'Tệp',
    'Bằng chuột',
    'Kết quả lệnh',
  ])
  // Every row of the sheet is on screen, in the table's Vietnamese.
  const rows = sheet().flatMap((group) => group.rows)
  expect(within(dialog).getAllByRole('term').map((term) => term.textContent).sort()).toEqual(rows.map(([, label]) => label).sort())
  expect(stray()).toEqual([])
  await user.type(within(dialog).getByRole('textbox', { name: 'Tìm phím tắt' }), 'zzz')
  expect(within(dialog).getByRole('status')).toHaveTextContent('Không có phím tắt nào khớp.')
  expect(stray()).toEqual([])
  await shut()
  // Half a chord, in Vietnamese.
  await user.keyboard(leader)
  expect(within(status()).getByRole('status')).toHaveTextContent('Ctrl+A đang chờ phím thứ hai — bấm ? để xem danh sách')
  expect(stray()).toEqual([])
  await user.keyboard('{Escape}')
  // Nowhere in the window is there a row of keys strung on dots, as a terminal's status line has.
  expect(document.body.textContent).not.toMatch(/\p{L}\s·\s<?\p{L}+\s\p{L}+\s·/u)
  expect(status().querySelector('kbd')).toBeNull()
})

// --- D10, M5: panes, focus mode, and the tabs under the pointer ---------------

/** The views on screen, by name. */
const panels = () => screen.getAllByRole('tabpanel').map((panel) => panel.getAttribute('aria-label'))
/** The box the panes are laid out in, and the panes on screen in their order. */
const frame = () => document.querySelector<HTMLElement>('[data-panes]')!
const laid = () =>
  [...frame().children]
    .filter((pane): pane is HTMLElement => pane instanceof HTMLElement && !pane.hidden && pane.querySelector('[role=tabpanel]') !== null)
    // In reading order: by where each is.
    .sort((a, b) => parseFloat(a.style.top) - parseFloat(b.style.top) || parseFloat(a.style.left) - parseFloat(b.style.left))
const order = () => laid().map((pane) => pane.querySelector('[role=tabpanel]')!.getAttribute('aria-label'))
/** Where each pane on screen is, in hundredths of the whole: left, top, width, height. */
const spots = () => laid().map((pane) => [pane.style.left, pane.style.top, pane.style.width, pane.style.height].map((part) => Math.round(parseFloat(part))))
const picked = () => screen.getByRole('tab', { selected: true })
const chord = (key: string) => `{Control>}{Shift>}${key}{/Shift}{/Control}`
/** The pointer with a tab in hand, over the panes; the box is a thousand by six hundred. */
const over = (type: 'dragenter' | 'dragover' | 'dragleave' | 'drop', clientX: number, clientY: number) => {
  vi.spyOn(frame(), 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 1000, height: 600 } as DOMRect)
  return fireEvent(frame(), new MouseEvent(type, { clientX, clientY, bubbles: true, cancelable: true }))
}
const connectTo = async (user: ReturnType<typeof shell>, alias: string) => {
  await user.keyboard(`${leader}0`)
  await user.click(within(await card(alias)).getByRole('button', { name: 'Connect' }))
}

test('Ctrl+Shift+D is another shell on the same host beside the one in front, Ctrl+Shift+E one under it; a pane is a tab like any other', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  expect(panels()).toEqual(['web-01'])
  await user.keyboard(chord('D'))
  await tab(/2 web-01/)
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }, { alias: 'web-01' }])
  // Both are on screen, side by side; the new one has the keys, and the line that says so.
  expect(panels()).toEqual(['web-01', 'web-01'])
  expect(frame()).not.toHaveAttribute('data-stacked')
  expect(picked()).toHaveTextContent('2 web-01')
  expect(terminals[1].focused).toBe(true)
  expect(laid().map((pane) => pane.hasAttribute('data-front'))).toEqual([false, true])
  expect((await tab(/1 web-01/)).parentElement).toHaveAttribute('data-pane')
  // A click in the other pane brings it to the front; so do the keys that go through the tabs.
  await fireEvent.pointerDown(laid()[0])
  expect(picked()).toHaveTextContent('1 web-01')
  expect(terminals[0].focused).toBe(true)
  await user.keyboard('{Control>}{Tab}{/Control}')
  expect(picked()).toHaveTextContent('2 web-01')
  expect(panels()).toHaveLength(2)
  // The dashboard is no pane: it is alone, and the split is there again behind its tabs.
  await user.keyboard(`${leader}0`)
  expect(screen.queryAllByRole('tabpanel')).toHaveLength(0)
  await user.keyboard(`${leader}1`)
  expect(panels()).toHaveLength(2)

  // One under the pane in front (the left one), which shares its side with it: two on the left, one on the right.
  await user.keyboard(chord('E'))
  await tab(/3 web-01/)
  expect(panels()).toHaveLength(3)
  expect(frame()).not.toHaveAttribute('data-stacked')
  expect(spots()).toEqual([[0, 0, 50, 50], [50, 0, 50, 100], [0, 50, 50, 50]])
  expect(terminals[2].focused).toBe(true)
  // Not one of these keys was a shell's.
  expect(asked('session_write')).toEqual([])

  // Closed, a pane leaves the front to one it was with; the last one left is a tab on its own.
  await user.keyboard(ctrlW)
  expect(panels()).toHaveLength(2)
  expect(picked()).toHaveTextContent('1 web-01')
  await user.click(screen.getByRole('button', { name: 'Unsplit: this pane back to a tab of its own' }))
  expect(panels()).toHaveLength(1)
  expect(document.querySelector('[data-pane]')).toBeNull()
  expect(screen.queryByRole('button', { name: /Unsplit/ })).toBeNull()
})

test('after the leader the split has tmux\'s keys: | and % beside, - and " under, ! out of it, { and } to change places', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  await user.dblClick(await tab(/1 web-01/))
  await user.keyboard('first{Enter}')
  for (const [key, stacked] of [['|', false], ['-', true], ['%', false], ['"', true]] as const) {
    await user.keyboard(`${leader}1${leader}${key === '"' ? '"' : key}`)
    await vi.waitFor(() => expect(panels()).toHaveLength(2))
    expect(frame().hasAttribute('data-stacked'), key).toBe(stacked)
    // Under it or beside it, the new one comes after.
    expect(order()).toEqual(['first', 'web-01'])
    await user.keyboard(ctrlW)
    expect(panels()).toEqual(['first'])
  }
  await user.keyboard(`${leader}|`)
  await vi.waitFor(() => expect(order()).toEqual(['first', 'web-01']))
  // The pane in front, one place along; at the end there is no further.
  await user.keyboard(`${leader}{{`)
  expect(order()).toEqual(['web-01', 'first'])
  await user.keyboard(`${leader}{{`)
  expect(order()).toEqual(['web-01', 'first'])
  await user.keyboard(`${leader}}`)
  expect(order()).toEqual(['first', 'web-01'])
  expect(picked()).toHaveTextContent('2 web-01')
  await user.keyboard(`${leader}!`)
  expect(panels()).toEqual(['web-01'])
  // Over the dashboard there is nothing to split, by key or by button.
  await user.keyboard(`${leader}0${chord('D')}${leader}|`)
  expect(asked('session_open')).toHaveLength(6)
})

test('beside a host\'s files a split is its shell; each pane takes the keys when it is the one in front', async () => {
  const user = shell()
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  await screen.findByRole('row', { name: /nginx/ })
  await user.click(screen.getByRole('button', { name: 'Split right' }))
  await tab(/1 web-01/)
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }])
  expect(order()).toEqual(['Files web-01', 'web-01'])
  expect(terminals[0].focused).toBe(true)
  expect(screen.getByRole('grid', { name: 'local' })).not.toHaveFocus()
  await fireEvent.pointerDown(laid()[0])
  expect(picked()).toHaveTextContent('2 Files web-01')
  await vi.waitFor(() => expect(screen.getByRole('grid', { name: 'local' })).toHaveFocus())
  // The keys in a pane are its own: the files' Ctrl+F is their filter, with a terminal beside them.
  await user.keyboard('{Control>}f{/Control}')
  expect(within(screen.getByRole('region', { name: 'local' })).getByRole('searchbox', { name: 'Filter' })).toHaveFocus()
})

test('a split that cannot open its shell says why and splits nothing', async () => {
  let opened = 0
  const user = shell({
    session_open: () => {
      if (opened++) throw refused
      return opened
    },
  })
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  await user.click(screen.getByRole('button', { name: 'Split down' }))
  await vi.waitFor(() => expect(opened).toBe(2))
  expect(panels()).toEqual(['web-01'])
  expect(screen.getAllByRole('tab')).toHaveLength(2)
  await user.keyboard(`${leader}0`)
  expect(await screen.findByText(/Connection refused/)).toBeInTheDocument()
})

test('a tab dragged to an edge of what is open becomes a pane there; dragged again, the panes change places', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await connectTo(user, 'web-02')
  await tab(/2 web-02/)
  expect(panels()).toEqual(['web-02'])
  // Nothing in hand: the panes are no place to drop.
  expect(await over('dragover', 50, 300)).toBe(true)
  expect(document.querySelector('[data-edge]')).toBeNull()

  // web-01's tab, to the left edge of web-02: the half it will take is shown, then it is there.
  await fireEvent.dragStart(await tab(/1 web-01/))
  expect(await over('dragover', 50, 300)).toBe(false)
  expect(document.querySelector('[data-edge]')).toHaveAttribute('data-edge', 'left')
  await over('drop', 50, 300)
  expect(document.querySelector('[data-edge]')).toBeNull()
  expect(order()).toEqual(['web-01', 'web-02'])
  expect(frame()).not.toHaveAttribute('data-stacked')
  expect(picked()).toHaveTextContent('1 web-01')
  expect(terminals[0].focused).toBe(true)

  // The same tab to the right edge: the two change places. On the way the pointer leaves one
  // pane for the next, which a web view says as a leave with nothing named (WebKit): the half
  // stays shown while the pointer is over the panes, and goes when it is outside them.
  await fireEvent.dragStart(await tab(/1 web-01/))
  await over('dragover', 960, 300)
  expect(document.querySelector('[data-edge]')).toHaveAttribute('data-edge', 'right')
  await over('dragleave', 960, 300)
  expect(document.querySelector('[data-edge]')).toHaveAttribute('data-edge', 'right')
  await over('dragleave', 1200, 300)
  expect(document.querySelector('[data-edge]')).toBeNull()
  // Let go with a leave as the last thing heard: it lands where it is let go.
  await over('drop', 960, 300)
  expect(order()).toEqual(['web-02', 'web-01'])
  // Over its own pane a tab would land where it is: no place is shown.
  await fireEvent.dragStart(await tab(/2 web-02/))
  expect(await over('dragover', 100, 300)).toBe(false)
  expect(document.querySelector('[data-edge]')).toBeNull()
  await fireEvent.dragEnd(await tab(/2 web-02/))
  // To the bottom of the other: one over the other. To its top: the other way round. A web view that last asked
  // on entering an element (over a terminal every row is one) is told yes there too, or it drops nothing.
  await fireEvent.dragStart(await tab(/2 web-02/))
  expect(await over('dragenter', 750, 590)).toBe(false)
  expect(document.querySelector('[data-edge]')).toHaveAttribute('data-edge', 'bottom')
  await over('drop', 750, 590)
  expect(frame()).toHaveAttribute('data-stacked')
  expect(order()).toEqual(['web-01', 'web-02'])
  await fireEvent.dragStart(await tab(/2 web-02/))
  await over('dragover', 500, 10)
  await over('drop', 500, 10)
  expect(order()).toEqual(['web-02', 'web-01'])

  // Let go anywhere else, the tab is where it was.
  await fireEvent.dragStart(await tab(/1 web-01/))
  await over('dragover', 50, 300)
  await fireEvent.dragEnd(await tab(/1 web-01/))
  expect(document.querySelector('[data-edge]')).toBeNull()
  expect(order()).toEqual(['web-02', 'web-01'])
  // A third tab joins the two; a host's files are a tab too.
  await user.keyboard(`${leader}0`)
  ;(await card('web-01')).focus()
  await user.keyboard(`f${leader}1`)
  // At the right edge of the lower pane: that one's half, and the upper pane keeps the width.
  await fireEvent.dragStart(await tab(/Files web-01/))
  await over('dragover', 990, 450)
  expect(document.querySelector<HTMLElement>('[data-edge]')!.style.cssText).toBe('left: 50%; top: 50%; width: 50%; height: 50%;')
  await over('drop', 990, 450)
  expect(order()).toEqual(['web-02', 'web-01', 'Files web-01'])
  expect(spots()).toEqual([[0, 0, 100, 50], [0, 50, 50, 50], [50, 50, 50, 50]])
  // In the middle of a pane: the two change places.
  await fireEvent.dragStart(await tab(/Files web-01/))
  await over('dragover', 500, 100)
  expect(document.querySelector('[data-edge]')).toHaveAttribute('data-edge', 'centre')
  await over('drop', 500, 100)
  expect(order()).toEqual(['Files web-01', 'web-01', 'web-02'])
  expect(picked()).toHaveTextContent('Files web-01')
  // A tab alone, dropped on itself, is still alone.
  await user.keyboard(`${leader}!`)
  expect(panels()).toEqual(['Files web-01'])
  await fireEvent.dragStart(await tab(/Files web-01/))
  await over('dragover', 50, 300)
  await over('drop', 50, 300)
  expect(panels()).toEqual(['Files web-01'])
  expect(order()).toEqual(['Files web-01'])
})

test('focus mode: the rail and the status bar go, a tab\'s view is all there is; Ctrl+B, the settings\' row, Ctrl+A z, and Esc back', async () => {
  const user = shell()
  await card('web-01')
  const rail = () => screen.queryByRole('navigation')
  const hide = () => setting(user, /Focus mode/)
  await hide()
  expect(rail()).toBeNull()
  expect(screen.queryByRole('contentinfo')).toBeNull()
  // The dashboard keeps its head and its cards; the strip of tabs is there for the way back.
  expect(screen.getByRole('heading', { name: 'Status' })).toBeInTheDocument()
  expect(await card('web-01')).toBeVisible()
  await user.click(screen.getByRole('button', { name: 'Show the sidebar' }))
  expect(rail()).toBeInTheDocument()
  expect(status()).toBeInTheDocument()
  expect(screen.queryByRole('tablist')).toBeNull()
  // By key, and Esc is the way out where no shell wants it.
  await user.keyboard('{Control>}b{/Control}')
  expect(rail()).toBeNull()
  await user.keyboard('{Escape}')
  expect(rail()).toBeInTheDocument()
  await user.keyboard('{Control>}b{/Control}{Control>}b{/Control}')
  expect(rail()).toBeInTheDocument()

  // In a terminal Ctrl+B is the shell's (a tmux over there has it for its prefix): with Shift it is the window's.
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  await user.keyboard('{Control>}b{/Control}')
  expect(rail()).toBeInTheDocument()
  await user.keyboard(chord('B'))
  expect(rail()).toBeNull()
  // With Caps Lock on, Shift gives the small letter: the same key.
  await fireEvent.keyDown(document.body, { key: 'b', code: 'KeyB', ctrlKey: true, shiftKey: true })
  expect(rail()).toBeInTheDocument()
  await user.keyboard(chord('B'))
  expect(rail()).toBeNull()
  expect(screen.queryByRole('contentinfo')).toBeNull()
  expect(screen.queryByRole('heading', { name: 'web-01' })).toBeNull()
  expect(screen.getByRole('tablist')).toBeInTheDocument()
  expect(panels()).toEqual(['web-01'])
  // Esc is vim's there.
  await user.keyboard('{Escape}')
  expect(rail()).toBeNull()
  // A button of the window had the focus, and went or stayed: the keys are the terminal's again.
  const keys = vi.spyOn(terminals[0], 'focus')
  await user.click(screen.getByRole('button', { name: 'Show the sidebar' }))
  expect(keys).toHaveBeenCalledTimes(1)
  await hide()
  expect(keys).toHaveBeenCalledTimes(2)
  expect(rail()).toBeNull()
  // Half a chord is said in the strip, where the status bar is not.
  await user.keyboard(leader)
  expect(within(screen.getByRole('tablist').parentElement!).getByRole('status')).toHaveTextContent('Ctrl+A')
  await user.keyboard('z')
  expect(rail()).toBeInTheDocument()
  await user.keyboard(`${leader}z`)
  expect(rail()).toBeNull()
  // From the palette too.
  await user.keyboard(ctrlK)
  await user.type(palette(), 'focus{Enter}')
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(rail()).toBeInTheDocument()
  expect(asked('session_write')).toEqual([])
})

test('a tab renamed by a double click is called that in its tab and the rail; with no name it is its host\'s again', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await user.dblClick(await tab(/1 web-01/))
  await user.keyboard('migration{Enter}')
  expect(await tab(/1 migration/)).toHaveAttribute('aria-selected', 'true')
  expect(within(screen.getByRole('list', { name: 'Sessions' })).getByRole('button')).toHaveTextContent('migration')
  expect(panels()).toEqual(['migration'])
  // The keys are the terminal's again.
  expect(terminals[0].focused).toBe(true)
  // Its host is still web-01: a split of it opens that.
  await user.keyboard(chord('D'))
  await tab(/2 web-01/)
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }, { alias: 'web-01' }])
  await user.dblClick(await tab(/1 migration/))
  await user.clear(screen.getByRole('textbox', { name: 'Rename tab' }))
  await user.keyboard('{Enter}')
  expect(await tab(/1 web-01/)).toBeInTheDocument()
  // The wheel's button closes a tab without bringing it to the front first.
  await fireEvent(await tab(/2 web-01/), new MouseEvent('auxclick', { button: 1, bubbles: true }))
  expect(asked('session_close')).toEqual([{ id: 2 }])
  expect(panels()).toEqual(['web-01'])
  // The + of the strip is a new tab: the palette asks where to.
  await user.click(screen.getByRole('button', { name: 'New tab: quick connect' }))
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
})

test('panes and focus mode in Vietnamese: the strip\'s buttons, the palette\'s rows and the rail\'s corner, with nothing in English', async () => {
  setLang('vi')
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Kết nối' }))
  await tab(/1 web-01/)
  expect(screen.getByRole('button', { name: 'Thu gọn thanh bên: chỉ biểu tượng' })).toHaveAttribute('aria-pressed', 'false')
  await user.click(screen.getByRole('button', { name: 'Chia đôi sang phải' }))
  await tab(/2 web-01/)
  expect(screen.getByRole('button', { name: 'Bỏ chia: đưa ô này về tab riêng' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Chia đôi xuống dưới' })).toHaveAttribute('title', 'Chia đôi xuống dưới (Ctrl+Shift+E)')
  expect(screen.getByRole('button', { name: 'Tab mới: kết nối nhanh' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  await user.dblClick(await tab(/1 web-01/))
  expect(screen.getByRole('textbox', { name: 'Đổi tên tab' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  await user.keyboard('{Escape}')
  await user.keyboard(chord('B'))
  expect(screen.getByRole('button', { name: 'Hiện thanh bên' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  // The palette has them by name, and by their English one.
  await user.keyboard(ctrlK)
  const rows = () => within(screen.getByRole('dialog')).getAllByRole('option').map((row) => row.textContent?.replace(/\s+/g, ' ').trim())
  expect(rows()).toContain('Chia đôi sang phải Ctrl+Shift+D')
  expect(rows()).toContain('Chia đôi xuống dưới Ctrl+Shift+E')
  expect(rows()).toContain('Chế độ tập trung Ctrl+Shift+B Ctrl+B')
  expect(stray()).toEqual([])
  await user.type(palette(), 'split down{Enter}')
  await tab(/3 web-01/)
  expect(spots()).toHaveLength(3)
  expect(screen.getAllByRole('separator', { name: 'Đổi cỡ các ô' })).toHaveLength(2)
  expect(stray()).toEqual([])
  // The cheatsheet's rows for them, found by a word of theirs.
  await user.keyboard(`${leader}?`)
  await user.type(within(screen.getByRole('dialog', { name: 'Phím tắt' })).getByRole('textbox'), 'chia')
  expect(within(screen.getByRole('dialog')).getAllByRole('term').map((term) => term.textContent)).toEqual([
    'Chia đôi sang phải',
    'Chia đôi xuống dưới',
    'Bỏ chia: đưa ô này về tab riêng',
    'Chia đôi sang phải',
    'Chia đôi xuống dưới',
    'Tab vào một mép của ô đang mở: chia ở phía đó, hoặc đổi chỗ',
  ])
  expect(stray()).toEqual([])
})

test('a split of a quick connect connects to the same address again, and that is a quick connect too', async () => {
  const user = shell()
  await card('web-01')
  await user.keyboard('{Control>}t{/Control}')
  await user.type(palette(), 'deploy@10.0.0.9{Enter}')
  await tab(/1 deploy@10.0.0.9/)
  await user.keyboard(chord('D'))
  await tab(/2 deploy@10.0.0.9/)
  expect(asked('quick_connect')).toEqual([{ host: 'deploy@10.0.0.9' }, { host: 'deploy@10.0.0.9' }])
  expect(asked('session_open')).toEqual([])
  expect(panels()).toHaveLength(2)
  // The new one is on no host of the config either: it can be kept as one.
  expect(picked()).toHaveTextContent('2 deploy@10.0.0.9')
  expect(within(status()).getByRole('button', { name: 'Save this host' })).toBeInTheDocument()
})

test('a tab whose shell is running something is asked about before it goes; Cancel comes first, so Enter is not a yes', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/web-01/)
  await says(1, { state: 'up' })
  await says(1, { state: 'busy', command: 'pg_dump' })
  // By the key, by the ×, by the leader's x: the same question, and nothing closed.
  await user.keyboard(ctrlW)
  const ask = await screen.findByRole('dialog', { name: 'Close tab' })
  expect(ask).toHaveTextContent('pg_dump is running on web-01. Close the tab?')
  // Cancel is the first button: a dialog opens with the focus on it (the web view's doing, not jsdom's).
  expect(within(ask).getAllByRole('button')[0]).toHaveTextContent('Cancel')
  await user.click(within(ask).getByRole('button', { name: 'Cancel' }))
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(asked('session_close')).toEqual([])
  // The keys are the terminal's again.
  expect(terminals[0].focused).toBe(true)
  await user.click(screen.getByRole('button', { name: 'close web-01' }))
  await user.click(within(await screen.findByRole('dialog', { name: 'Close tab' })).getByRole('button', { name: 'Close tab' }))
  expect(asked('session_close')).toEqual([{ id: 1 }])
  expect(screen.queryByRole('tablist')).not.toBeInTheDocument()

  // At its prompt a tab goes at once, as it did; so does one whose session is over.
  await user.click(within(await card('web-02')).getByRole('button', { name: 'Connect' }))
  await says(2, { state: 'up' })
  await says(2, { state: 'busy', command: 'top' })
  await says(2, { state: 'closed', error: null })
  await user.keyboard(ctrlW)
  expect(asked('session_close')).toEqual([{ id: 1 }, { id: 2 }])
  expect(screen.queryByRole('dialog')).toBeNull()
})

test('the question before a busy tab goes, in Vietnamese', async () => {
  setLang('vi')
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Kết nối' }))
  await says(1, { state: 'up' })
  put.push('pg_dump')
  await says(1, { state: 'busy', command: 'pg_dump' })
  await user.keyboard(`${leader}x`)
  expect(await screen.findByRole('dialog', { name: 'Đóng tab' })).toHaveTextContent('pg_dump đang chạy trên web-01. Đóng tab?')
  expect(stray()).toEqual([])
})

// --- D11: the pointer's own ways ------------------------------------------------

const rightClick = (target: Element) => fireEvent.contextMenu(target, { clientX: 300, clientY: 200 })
const offered = () => within(screen.getByRole('menu')).getAllByRole('menuitem').map((item) => item.textContent?.replace(/\s+/g, ' ').trim())

test('the line between two panes can be held and moved, by the pointer or the arrows; never to nothing', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  // One tab alone: no line.
  expect(screen.queryByRole('separator')).toBeNull()
  await user.keyboard(chord('D'))
  await tab(/2 web-01/)
  const line = () => screen.getByRole('separator', { name: 'Resize the panes' })
  expect(line()).toHaveAttribute('aria-orientation', 'vertical')
  expect(spots()).toEqual([[0, 0, 50, 100], [50, 0, 50, 100]])
  vi.spyOn(frame(), 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 1000, height: 600 } as DOMRect)
  // Held at 500 of 1000 and taken to 700: the left pane has seven tenths.
  await fireEvent.pointerDown(line(), { clientX: 500, clientY: 300 })
  await fireEvent.pointerMove(line(), { clientX: 600, clientY: 300 })
  await fireEvent.pointerMove(line(), { clientX: 700, clientY: 310 })
  expect(spots()).toEqual([[0, 0, 70, 100], [70, 0, 30, 100]])
  // Past the edge: the other pane keeps a strip. Let go, the keys are the terminal's again.
  await fireEvent.pointerMove(line(), { clientX: 1500, clientY: 300 })
  expect(spots()).toEqual([[0, 0, 88, 100], [88, 0, 12, 100]])
  await fireEvent.pointerUp(line(), { clientX: 1500, clientY: 300 })
  expect(terminals[1].focused).toBe(true)
  // Not held: the pointer passing over it moves nothing.
  await fireEvent.pointerMove(line(), { clientX: 200, clientY: 300 })
  expect(spots()[0][2]).toBe(88)
  // By the keys, a twentieth a press; the other two arrows are not this line's.
  line().focus()
  await user.keyboard('{ArrowLeft}{ArrowLeft}{ArrowUp}')
  expect(spots()).toEqual([[0, 0, 78, 100], [78, 0, 22, 100]])
  // A pane under the one in front: a second line, lying down, for those two.
  await user.keyboard(chord('E'))
  await tab(/3 web-01/)
  expect(screen.getAllByRole('separator').map((bar) => bar.getAttribute('aria-orientation'))).toEqual(['vertical', 'horizontal'])
  expect(spots()).toEqual([[0, 0, 78, 100], [78, 0, 22, 50], [78, 50, 22, 50]])
})

test('a tab let go on another in the strip stands there; the digits and the keys follow the strip', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await connectTo(user, 'web-02')
  await connectTo(user, 'db-primary')
  await tab(/3 db-primary/)
  const strip = () => screen.getAllByRole('tab').map((tab) => tab.textContent?.replace(/\s+/g, ' ').trim().replace(/^[●○] /, ''))
  expect(strip()).toEqual(['0 Hosts', '1 web-01', '2 web-02', '3 db-primary'])
  // db-primary's tab, let go on the left half of web-01's: before it.
  const pill = async (name: RegExp) => (await tab(name)).parentElement!
  /** Over a tab, left or right of its middle (a box of no size, at nought). */
  const onto = async (type: 'dragover' | 'drop', name: RegExp, clientX: number) =>
    fireEvent(await pill(name), new MouseEvent(type, { clientX, bubbles: true, cancelable: true }))
  await fireEvent.dragStart(await pill(/3 db-primary/))
  await onto('dragover', /1 web-01/, -5)
  expect(await pill(/1 web-01/)).toHaveAttribute('data-drop', 'before')
  expect(await pill(/3 db-primary/)).toHaveAttribute('data-dragging')
  await onto('drop', /1 web-01/, -5)
  expect(strip()).toEqual(['0 Hosts', '1 db-primary', '2 web-01', '3 web-02'])
  expect(document.querySelector('[data-drop], [data-dragging]')).toBeNull()
  // On the right half of the last: after it.
  await fireEvent.dragStart(await pill(/1 db-primary/))
  await onto('dragover', /3 web-02/, 5)
  expect(await pill(/3 web-02/)).toHaveAttribute('data-drop', 'after')
  await onto('drop', /3 web-02/, 5)
  expect(strip()).toEqual(['0 Hosts', '1 web-01', '2 web-02', '3 db-primary'])
  await fireEvent.dragStart(await pill(/2 web-02/))
  await onto('drop', /3 db-primary/, 5)
  expect(strip()).toEqual(['0 Hosts', '1 web-01', '2 db-primary', '3 web-02'])
  // The leader's digits and Ctrl+Tab go by the strip, not by which was opened first.
  await user.keyboard(`${leader}2`)
  expect(picked()).toHaveTextContent('2 db-primary')
  await user.keyboard('{Control>}{Tab}{/Control}')
  expect(picked()).toHaveTextContent('3 web-02')
  // A tab opened later comes last.
  await connectTo(user, 'web-01')
  await tab(/4 web-01/)
  expect(strip()).toEqual(['0 Hosts', '1 web-01', '2 db-primary', '3 web-02', '4 web-01'])
})

test('the right button on a tab: close it, close the others (a busy one is asked about), rename it, split it', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await connectTo(user, 'web-02')
  await connectTo(user, 'db-primary')
  await tab(/3 db-primary/)
  await rightClick(await tab(/1 web-01/))
  expect(offered()).toEqual(['Close', 'Close other tabs', 'Rename tab', 'Split right Ctrl+Shift+D', 'Split down Ctrl+Shift+E'])
  // Esc closes the menu and nothing else.
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('menu')).toBeNull()
  expect(picked()).toHaveTextContent('3 db-primary')
  // Split from the menu: the tab comes to the front first, and it is that one's host.
  await rightClick(await tab(/1 web-01/))
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: /Split right/ }))
  await tab(/4 web-01/)
  expect(order()).toEqual(['web-01', 'web-01'])
  expect(asked('session_open').at(-1)).toEqual({ alias: 'web-01' })
  // On a pane the menu has the way out of the split.
  await rightClick(await tab(/4 web-01/))
  expect(offered()).toContain('Unsplit: this pane back to a tab of its own')
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: /Unsplit/ }))
  expect(panels()).toEqual(['web-01'])
  await rightClick(await tab(/2 web-02/))
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Rename tab' }))
  await user.keyboard('staging{Enter}')
  await tab(/2 staging/)
  // Every tab but this one goes; one whose shell runs something stays until somebody says yes.
  await says(3, { state: 'up' })
  await says(3, { state: 'busy', command: 'make' })
  await rightClick(await tab(/2 staging/))
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Close other tabs' }))
  expect(screen.getAllByRole('tab').map((tab) => tab.textContent?.replace(/\s+/g, ' ').trim().replace(/^[●○] /, ''))).toEqual(['0 Hosts', '1 staging', '2 db-primary running make'])
  expect(screen.getByRole('dialog', { name: 'Close tab' })).toHaveTextContent('make is running on db-primary. Close the tab?')
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close tab' }))
  expect(screen.getAllByRole('tab')).toHaveLength(2)
  expect(picked()).toHaveTextContent('1 staging')
  expect(asked('session_close').map((args) => args.id).sort()).toEqual([1, 3, 4])
  // A file view's tab: no name of its own to give.
  await user.keyboard(`${leader}0`)
  ;(await card('web-01')).focus()
  await user.keyboard('f')
  await rightClick(await tab(/Files web-01/))
  expect(offered()).not.toContain('Rename tab')
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Close' }))
  expect(screen.queryByRole('tab', { name: /Files/ })).toBeNull()
})

test('the right button in a terminal: the selection to the clipboard, the clipboard to the shell, all of it selected', async () => {
  const user = shell({ 'plugin:clipboard-manager|read_text': () => 'uptime' })
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  await says(1, { state: 'up' })
  await prints(1, 'load average: 0.1')
  const panel = screen.getByRole('tabpanel', { name: 'web-01' })
  await rightClick(panel)
  expect(offered()).toEqual(['Copy', 'Paste', 'Select all'])
  // Nothing is selected yet: nothing to copy.
  expect(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Copy' })).toBeDisabled()
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Select all' }))
  expect(screen.queryByRole('menu')).toBeNull()
  expect(terminals[0].focused).toBe(true)
  const before = copied().length
  await rightClick(panel)
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Copy' }))
  expect(copied().slice(before)).toEqual(['load average: 0.1'])
  // Paste types what the clipboard holds, into this shell.
  await rightClick(panel)
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: 'Paste' }))
  await vi.waitFor(() => expect(typed()).toContain('1:uptime'))
})

test('the right button anywhere else is no menu of the web view\'s (its Reload would end every session); a field keeps its own', async () => {
  shell()
  await card('web-01')
  expect(await fireEvent.contextMenu(screen.getByRole('contentinfo'))).toBe(false)
  expect(await fireEvent.contextMenu(screen.getByRole('searchbox', { name: /Search hosts/ }))).toBe(true)
})

test('the menus in Vietnamese: a tab\'s, a card\'s and a terminal\'s, with nothing in English', async () => {
  setLang('vi')
  const user = shell()
  await rightClick(await card('web-01'))
  expect(offered()).toEqual(['Kết nối Enter', 'Tệp F', 'Chuyển tiếp L', 'Sửa E', 'Chép địa chỉ', 'Bảng lệnh Ctrl+K', 'Xoá D'])
  expect(stray()).toEqual([])
  await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: /Kết nối/ }))
  await tab(/1 web-01/)
  await rightClick(await tab(/1 web-01/))
  expect(offered()).toEqual(['Đóng Ctrl+Shift+W', 'Đóng các tab khác', 'Đổi tên tab', 'Chia đôi sang phải Ctrl+Shift+D', 'Chia đôi xuống dưới Ctrl+Shift+E'])
  expect(stray()).toEqual([])
  await user.keyboard('{Escape}')
  await rightClick(screen.getByRole('tabpanel', { name: 'web-01' }))
  expect(offered()).toEqual(['Sao chép', 'Dán', 'Chọn tất cả'])
  expect(stray()).toEqual([])
})

// --- D11: the rail narrow, and what was open, open again ------------------------

/** The app closed and opened again: the web view kept what it kept. */
async function again(over: Record<string, (args: Args) => unknown> = {}) {
  cleanup()
  await new Promise((done) => setTimeout(done))
  terminals.length = 0
  calls = []
  return shell(over)
}
const columns = () => screen.getByRole('main').parentElement!.className.match(/grid-cols-\[(\d+px)/)?.[1]

test('the rail narrow: its corner\'s button leaves the icons, 56 pixels of them, and the next time the app opens it is as it was left', async () => {
  const user = shell()
  await card('web-01')
  expect(columns()).toBe('244px')
  await user.click(rail('Compact sidebar: icons only'))
  expect(columns()).toBe('56px')
  expect(localStorage.getItem('vssh.rail')).toBe('compact')
  // Its rows are there by name, and go where they went; the hosts are the dashboard's.
  expect(within(screen.getAllByRole('navigation')[0]).queryByRole('list')).toBeNull()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  await user.click(rail('Status'))
  expect(await card('web-01')).toBeVisible()
  await user.click(rail('Sessions 1'))
  expect(await tab(/1 web-01/)).toHaveAttribute('aria-selected', 'true')
  // Focus mode takes even that, and gives it back as it was.
  await user.keyboard(`${chord('B')}${chord('B')}`)
  expect(columns()).toBe('56px')

  await again()
  await card('web-01')
  expect(columns()).toBe('56px')
  await user.click(rail('Expand the sidebar'))
  expect(columns()).toBe('244px')
  expect(localStorage.getItem('vssh.rail')).toBe('full')
  expect(screen.getByRole('list', { name: 'Hosts' })).toBeInTheDocument()
})

test('what was open is open the next time: the tabs of the config\'s hosts in their order, a tab\'s name, the split and its sizes, focus mode; a quick connect is not kept', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await user.dblClick(await tab(/1 web-01/))
  await user.keyboard('migration{Enter}')
  await user.keyboard(chord('D'))
  await tab(/2 web-01/)
  screen.getByRole('separator').focus()
  await user.keyboard('{ArrowRight}{ArrowRight}')
  await user.keyboard(`${leader}0`)
  ;(await card('web-02')).focus()
  await user.keyboard('f')
  await tab(/Files web-02/)
  await user.keyboard(`${leader}0{Control>}t{/Control}`)
  await user.type(palette(), 'deploy@10.0.0.9{Enter}')
  await tab(/deploy@10.0.0.9/)
  await user.click(await tab(/1 migration/))
  await user.keyboard(chord('B'))
  expect(screen.queryByRole('navigation')).toBeNull()
  const sizes = spots()
  expect(sizes.map((spot) => spot[2])).toEqual([60, 40])
  expect(JSON.parse(localStorage.getItem('vssh.workspace')!)).toMatchObject({
    zen: true,
    tabs: [{ host: 'web-01', name: 'migration' }, { host: 'web-01' }, { host: 'web-02', files: true }],
    active: 0,
  })

  await again()
  // One after the other, as each is opened; then the one that was in front is.
  await vi.waitFor(() => expect(picked()).toHaveTextContent('1 migration'))
  expect(screen.getAllByRole('tab').map((tab) => tab.textContent?.match(/\d \S+/)?.[0])).toEqual(['0 Hosts', '1 migration', '2 web-01', '3 Files'])
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }, { alias: 'web-01' }])
  expect(asked('quick_connect')).toEqual([])
  expect(order()).toEqual(['migration', 'web-01'])
  expect(spots()).toEqual(sizes)
  expect(screen.queryByRole('navigation')).toBeNull()
})

test('a host that is gone since is not opened, and its pane is not there; what the web view kept and is not this is nothing', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  await tab(/1 web-01/)
  await user.keyboard(`${leader}0`)
  await user.click(within(await card('web-02')).getByRole('button', { name: 'Connect' }))
  await tab(/2 web-02/)
  await fireEvent.dragStart(await tab(/1 web-01/))
  over('dragenter', 10, 50)
  over('drop', 10, 50)
  expect(order()).toEqual(['web-01', 'web-02'])

  await again({
    session_open: ({ alias }) => {
      if (alias === 'web-01') throw refused
      return 5
    },
  })
  // The one in front was the one that is gone: the dashboard is, and the other is a tab by itself.
  await user.click(await tab(/1 web-02/))
  expect(screen.getAllByRole('tab')).toHaveLength(2)
  expect(panels()).toEqual(['web-02'])
  expect(screen.queryByRole('separator')).toBeNull()

  localStorage.setItem('vssh.workspace', '{"tabs": 7}')
  await again()
  await card('web-01')
  expect(screen.queryByRole('tablist')).toBeNull()
  // And from then on what is open is written down as before.
  await user.click(within(await card('bare')).getByRole('button', { name: 'Connect' }))
  await tab(/1 bare/)
  expect(JSON.parse(localStorage.getItem('vssh.workspace')!).tabs).toEqual([{ host: 'bare' }])
})

test('with a vault that is locked, what was open waits for the answer: a session may need a password of it', async () => {
  localStorage.setItem('vssh.workspace', JSON.stringify({ zen: false, tabs: [{ host: 'web-01' }], layout: { stacked: false, groups: [], size: {} }, active: 0 }))
  shell({ vault_state: () => 'locked' })
  await screen.findByRole('dialog', { name: 'Vault' })
  await card('web-01')
  expect(asked('session_open')).toEqual([])
  await shut()
  expect(await tab(/1 web-01/)).toHaveAttribute('aria-selected', 'true')
  expect(asked('session_open')).toEqual([{ alias: 'web-01' }])
})
