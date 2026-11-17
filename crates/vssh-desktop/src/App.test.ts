import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.svelte'
import errors from './lib/fixtures/errors.json'
import hosts from './lib/fixtures/hosts.json'
import peek from './lib/fixtures/peek.json'
import refused from './lib/fixtures/refused.json'
import shown from './lib/fixtures/shown.json'
import { terminals } from './lib/fixtures/xterm'
import { setLang } from './lib/i18n.svelte'
import { forget } from './lib/live.svelte'
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
    'plugin:clipboard-manager|write_text': () => null,
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
  expect(status()).toHaveTextContent('Ctrl-A 0-9 tab · Tab next · y copy output')
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

test('the check itself: it finds English where there is some', () => {
  expect(english('Máy chủ 4 · Sort')).toEqual(['sort'])
  expect(english('CPU 23% 4s ago')).toEqual(['secs_ago'])
  expect(english('không có gì: Hostname, sorted, downtime')).toEqual([])
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
  expect(english(words())).toEqual([])

  // A session, and the quick peek of a host that answered and of one that is down.
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Kết nối' }))
  await screen.findByRole('tablist', { name: 'Phiên' })
  await says(1, { state: 'busy', command: 'top' })
  expect(await tab(/web-01/)).toHaveTextContent('đang chạy top')
  expect(english(words())).toEqual([])
  await says(1, { state: 'closed', error: refused })
  await screen.findByRole('button', { name: 'Kết nối lại' })
  expect(english(words())).toEqual([])
  await user.keyboard(`${leader}0`)
  ;(await card('web-02')).focus()
  await user.keyboard(' ')
  expect(await card('web-02')).toHaveAttribute('data-marked')
  expect(english(words())).toEqual([])
  await user.keyboard(' ')
  ;(await card('web-01')).focus()
  await user.keyboard('v')
  expect(screen.getByRole('dialog', { name: 'Xem nhanh: web-01' })).toHaveTextContent('Đã chạy')
  expect(english(words())).toEqual([])
  ;(await card('web-02')).focus()
  await user.keyboard('v')
  expect(await screen.findByRole('dialog', { name: 'Xem nhanh: web-02' })).toHaveTextContent('thử lại')
  expect(english(words())).toEqual([])
  await user.keyboard('{Escape}')

  // A search that finds nothing, the palette, the quick-connect form.
  await user.keyboard('/zzz')
  expect(screen.getByText('Không có máy chủ nào khớp.')).toBeInTheDocument()
  expect(english(words())).toEqual([])
  await user.keyboard('{Escape}{Control>}k{/Control}')
  expect(screen.getByRole('dialog', { name: 'Bảng lệnh' })).toBeInTheDocument()
  expect(english(words())).toEqual([])
  await user.type(palette(), 'qc{Enter}')
  expect(screen.getByRole('dialog', { name: 'Kết nối nhanh' })).toBeInTheDocument()
  expect(english(words())).toEqual([])
})

test('with no hosts, in Vietnamese, the empty dashboard is Vietnamese too', async () => {
  setLang('vi')
  shell({ list_hosts: () => [], list_snippets: () => [] })
  expect(await screen.findByRole('heading', { name: 'Chưa có máy chủ nào.' })).toBeInTheDocument()
  expect(english(words())).toEqual([])
})

test('every string of the table is one the window uses', async () => {
  const sources = import.meta.glob(['./**/*.svelte', '!./**/*.test.ts'], { query: '?raw', import: 'default', eager: true })
  const code = Object.values(sources).join('\n')
  const unused = Object.keys(strings.en).filter((key) => !code.includes(`t('${key}')`))
  expect(unused).toEqual([])
})
