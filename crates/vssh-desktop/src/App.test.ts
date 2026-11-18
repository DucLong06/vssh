import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
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
const fails = () => {
  throw refused
}
const tab = (name: RegExp) => screen.findByRole('tab', { name })
/** What a session's shell prints, and what it says about itself. */
const prints = (id: number, text: string) => emit(`pty-data:${id}`, [...new TextEncoder().encode(text)])
const says = (id: number, state: object) => emit(`pty-state:${id}`, state)
const copied = () => asked('plugin:clipboard-manager|write_text').map((args) => args.text)
const leader = '{Control>}a{/Control}'

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

  await user.click(screen.getByRole('button', { name: 'Language: EN' }))
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
  // The menu is on screen with a terminal, and lit while the next key is a command.
  expect(status()).toHaveTextContent('Ctrl-A 0-9 tab · Tab next · y copy output · b broadcast · x close')
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
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'db{Enter}')
  expect(picked()).toHaveTextContent('0 Hosts')
  await vi.waitFor(async () => expect(await card('db-primary')).toHaveFocus())
  // Chosen again from a terminal, the same host has its card focused again.
  await user.keyboard(`${leader}2{Control>}k{/Control}`)
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
  // On its own Ctrl-W closes the tab in front, at once; Ctrl-A x is the TUI's key for it.
  await user.keyboard('{Control>}w{/Control}')
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
  await user.click(screen.getByRole('button', { name: 'Language: EN' }))
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
  expect(within(status()).getByRole('status')).toHaveTextContent('Ctrl-A 0-9 tab')
  await user.keyboard('{Escape}')
  expect(within(status()).queryByRole('status')).not.toBeInTheDocument()
  // In a field Ctrl-A is the field's: it selects the text.
  await user.type(screen.getByRole('searchbox'), `web${leader}`)
  expect(within(status()).queryByRole('status')).not.toBeInTheDocument()
})

test('a host chosen in the palette is the selected one', async () => {
  const user = shell()
  await screen.findByRole('button', { name: /db-primary/ })
  // With no host selected there is nothing for the keys to act on.
  expect(status()).not.toHaveTextContent('Enter')
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'db{Enter}')
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: /db-primary/ })).toHaveAttribute('aria-current', 'true')
  expect(status()).toHaveTextContent('db-primary postgres@db.internal.example.com')
  await vi.waitFor(async () => expect(await card('db-primary')).toHaveFocus())
  // What the selected card's keys do, in the window's language.
  expect(status()).toHaveTextContent('Enter Connect · Space select · v Quick peek')
  await user.click(screen.getByRole('button', { name: 'Language: EN' }))
  expect(status()).toHaveTextContent('Enter Kết nối · Space chọn · v Xem nhanh')
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
  await user.type(palette(), 'qc{Enter}')
  await user.type(screen.getByLabelText('Host'), '10.0.0.9')
  await user.type(screen.getByLabelText('User'), 'deploy')
  // Behind a modal dialog the cards and their buttons are inert; jsdom does not know.
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Connect' }))

  expect(await tab(/deploy@10.0.0.9/)).toHaveAttribute('aria-selected', 'true')
  await vi.waitFor(() => expect(asked('session_start')).toEqual([{ id: 7, cols: 80, rows: 24 }]))
  expect(sent).toEqual([{ host: 'deploy@10.0.0.9' }])
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(status()).toHaveTextContent('Sessions 1')
  expect(status()).toHaveTextContent('Hosts 4')
})

test('a refused quick connect says why, in the language of the window', async () => {
  const user = shell({ quick_connect: fails })
  await user.click(await screen.findByRole('button', { name: /Language/ }))
  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'qc{Enter}')
  await user.type(screen.getByRole('textbox', { name: 'Máy chủ' }), 'web')
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Kết nối' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('BỊ TỪ CHỐI 10.0.0.5:2222 từ chối kết nối')
  expect(screen.getByRole('dialog', { name: 'Kết nối nhanh' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  expect(status()).toHaveTextContent('Phiên 0')
})

test('the language toggle changes every word and is remembered', async () => {
  const user = shell()
  await user.click(await screen.findByRole('button', { name: 'Language: EN' }))
  expect(screen.getByRole('heading', { name: 'Máy chủ 4' })).toBeInTheDocument()
  expect(screen.getAllByRole('button', { name: 'Kết nối' })).toHaveLength(4)
  expect(status()).toHaveTextContent('Phiên 0')
  expect(localStorage.getItem('vssh.lang')).toBe('vi')
  expect(document.documentElement.lang).toBe('vi')
  await user.click(screen.getByRole('button', { name: 'Ngôn ngữ: VI' }))
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
  ...['Ctrl-K', 'Ctrl-A', 'Enter', 'Space', 'enter', 'esc', 'CPU', 'ms', 'EN', 'VI'],
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
  // Also in small letters: a key's hint is the table's word without its capital.
  const table = Object.values(strings.vi).flatMap((vi) => vi.split(/\{\w+\}/)).flatMap((piece) => [piece, piece.toLowerCase()])
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
  expect(foreign('Máy chủ 4 · Thẻ 2 · web-01 deploy@10.0.0.5:2222 · CPU 23% · 12 ms · Ctrl-K · 4 giây trước')).toEqual([])
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

  // A search that finds nothing, the palette, the quick-connect form.
  await user.keyboard('/zzz')
  expect(screen.getByText('Không có máy chủ nào khớp.')).toBeInTheDocument()
  expect(stray()).toEqual([])
  await user.keyboard('{Escape}{Control>}k{/Control}')
  expect(screen.getByRole('dialog', { name: 'Bảng lệnh' })).toBeInTheDocument()
  expect(stray()).toEqual([])
  await user.type(palette(), 'qc{Enter}')
  expect(screen.getByRole('dialog', { name: 'Kết nối nhanh' })).toBeInTheDocument()
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
  // The keys are the file view's, and the status bar says which.
  expect(screen.getByRole('grid', { name: 'local' })).toHaveFocus()
  expect(status()).toHaveTextContent('Space select · c Copy · / Filter · . Hidden files · right-click more')
  // The keys stand out, as on the dashboard: two of them are punctuation.
  expect([...status().querySelectorAll('kbd')].map((key) => key.textContent)).toEqual(['Space', 'c', '/', '.'])
  expect(status()).not.toHaveTextContent('Ctrl-A')
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
  // The terminal's keys, not the file view's.
  expect(status()).toHaveTextContent('Ctrl-A')
  await user.keyboard(`${leader}2`)
  expect(await tab(/2 Files web-01/)).toHaveAttribute('aria-selected', 'true')
  setLang('vi')
  expect(await tab(/2 Tệp web-01/)).toBeInTheDocument()
  expect(status()).toHaveTextContent('Space chọn · c Sao chép · / Lọc · . Tệp ẩn · chuột phải thao tác khác')
  await user.keyboard(`${leader}0`)
  expect(within(await card('web-01')).getByRole('button', { name: 'Tệp' })).toBeInTheDocument()
  ;(await card('web-01')).focus()
  await vi.waitFor(() => expect(status()).toHaveTextContent('f Tệp'))
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
  await vi.waitFor(() => expect(status()).toHaveTextContent('x Run'))
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
  await vi.waitFor(() => expect(status()).toHaveTextContent('L Forwards'))
  await user.keyboard('L')
  const dialog = await screen.findByRole('dialog', { name: 'Forwards web-02' })
  expect(asked('forwards_list')).toEqual([{ host: 'web-02' }])
  const web = await within(dialog).findByRole('switch', { name: 'L 8080 localhost:80' })
  await user.click(web)
  expect(asked('forward_toggle')).toEqual([{ host: 'web-02', kind: 'L', value: '8080 localhost:80' }])
  await emit('forward-state', { host: 'web-02', kind: 'L', value: '8080 localhost:80', state: 'on', error: null })
  await vi.waitFor(() => expect(web).toHaveTextContent('● On'))
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

  await user.click(within(await card('web-01')).getByRole('button', { name: 'Forwards' }))
  expect(await screen.findByRole('dialog', { name: 'Forwards web-01' })).toBeInTheDocument()
  setLang('vi')
  expect(await screen.findByRole('dialog', { name: 'Chuyển tiếp web-01' })).toBeInTheDocument()
  await vi.waitFor(() => expect(status()).toHaveTextContent('L Chuyển tiếp'))
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
  await vi.waitFor(() => expect(status()).toHaveTextContent('e Edit'))
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

  await user.keyboard('{Control>}k{/Control}')
  await user.type(palette(), 'qc{Enter}')
  const quick = await screen.findByRole('dialog', { name: 'Quick connect' })
  await user.type(within(quick).getByLabelText('Host'), 'sandbox.example')
  await user.type(within(quick).getByLabelText('User'), 'root')
  await user.type(within(quick).getByLabelText('Port'), '2200')
  await user.click(within(quick).getByRole('button', { name: 'Connect' }))
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

  const toggle = screen.getByRole('button', { name: /Streamer mode/ })
  expect(toggle).toHaveAttribute('aria-pressed', 'false')
  await user.click(toggle)
  expect(toggle).toHaveAttribute('aria-pressed', 'true')
  expect(localStorage.getItem('vssh.streamer_mode')).toBe('true')

  // The cards, a failure's words on one, the sidebar, the status bar.
  expect(await card('web-01')).toHaveTextContent(`deploy@${one}:2222`)
  await vi.waitFor(async () => expect(await card('web-02')).toHaveTextContent(`REFUSED Connection refused by ${one}:2222`))
  expect(await card('web-02')).toHaveTextContent(`deploy@${two}`)
  expect(screen.getByRole('navigation')).toHaveTextContent(`postgres@${fake('db-primary')}`)
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
  await user.type(palette(), 'qc{Enter}')
  const quick = await screen.findByRole('dialog', { name: 'Quick connect' })
  expect(within(quick).getByLabelText('Host')).toHaveAttribute('type', 'password')
  await user.type(within(quick).getByLabelText('Host'), '10.0.0.9')
  await user.type(within(quick).getByLabelText('User'), 'deploy')
  await user.click(within(quick).getByRole('button', { name: 'Connect' }))
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
  await user.click(toggle)
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
  shell()
  expect(await card('web-01')).toHaveTextContent(`deploy@${fake('web-01')}:2222`)
  expect(screen.getByRole('button', { name: /Chế độ phát trực tiếp/ })).toHaveAttribute('aria-pressed', 'true')
  expect(stray()).toEqual([])
})

// --- D7: the Day theme --------------------------------------------------------

test('the theme toggle in the sidebar: Night until Day is chosen, a terminal dark in both', async () => {
  const user = shell()
  const toggle = screen.getByRole('button', { name: 'Theme: Night' })
  expect(document.documentElement.dataset.theme).toBe('night')
  await user.click(toggle)
  expect(toggle).toHaveAccessibleName('Theme: Day')
  expect(document.documentElement.dataset.theme).toBe('day')
  expect(localStorage.getItem('vssh.theme')).toBe('day')

  ;(await card('web-01')).focus()
  await user.keyboard('{Enter}')
  await tab(/web-01/)
  expect(terminals[0].options.theme).toMatchObject({ background: '#1a1b26', foreground: '#c0caf5' })
  // Its ground is the terminal's own, not the window's; the border is the theme's.
  expect(screen.getByRole('tabpanel', { name: 'web-01' })).toHaveClass('bg-term', 'day:border-edge')

  setLang('vi')
  await vi.waitFor(() => expect(toggle).toHaveAccessibleName('Giao diện: Sáng'))
  await user.click(toggle)
  expect(toggle).toHaveAccessibleName('Giao diện: Tối')
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
  // The terminal is in front: the head of the page is its host, and the rail says where one is.
  expect(screen.getByRole('heading', { level: 1, name: 'web-02' })).toBeInTheDocument()
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
  expect(screen.getByRole('heading', { level: 1, name: 'Files' })).toBeInTheDocument()
  expect(rail('Files')).toHaveAttribute('aria-current', 'page')
  expect(asked('sftp_realpath')).toEqual([{ host: 'web-02', path: '' }])

  await user.click(rail('Forwards'))
  expect(await screen.findByRole('dialog', { name: 'Forwards web-02' })).toBeInTheDocument()
  await shut()
  await user.click(rail(/Snippets/))
  expect(await screen.findByRole('dialog', { name: 'Snippets' })).toBeInTheDocument()
})

test('the top bar: the search opens the palette, and the line under Status counts who answers', async () => {
  const user = shell({
    get_host_stats: ({ alias }) => {
      if (alias === 'web-02') throw refused
      return peek
    },
  })
  await vi.waitFor(() => expect(head()).toHaveTextContent('4 Hosts · 3 reachable · 1 down'))
  await user.click(within(head()).getByRole('button', { name: 'Command palette' }))
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
