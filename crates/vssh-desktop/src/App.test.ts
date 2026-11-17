import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.svelte'
import errors from './lib/fixtures/errors.json'
import hosts from './lib/fixtures/hosts.json'
import peek from './lib/fixtures/peek.json'
import refused from './lib/fixtures/refused.json'
import shown from './lib/fixtures/shown.json'
import { forget } from './lib/live.svelte'

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
    connect_host: () => ++sessions,
    quick_connect: () => ++sessions,
    // The core's own words for a failure of that class.
    resolve_conn_error: ({ error, lang }) =>
      shown[lang as 'en' | 'vi'][errors.findIndex((known) => known.class === (error as typeof refused).class)],
    ...over,
  }
  mockIPC((cmd, args) => {
    calls.push([cmd, args as Args])
    return commands[cmd](args as Args)
  })
  render(App)
  // Under fake timers the user's own pauses are on the fake clock too.
  return userEvent.setup({ advanceTimers: (ms) => void (vi.isFakeTimers() && vi.advanceTimersByTime(ms)) })
}
const status = () => screen.getByRole('contentinfo')
const card = (alias: string) => screen.findByRole('article', { name: alias })
const fails = () => {
  throw refused
}

afterEach(() => {
  vi.useRealTimers()
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

test('Connect on a card opens a session that shows the host and its figures', async () => {
  const user = shell()
  await user.click(within(await card('web-01')).getByRole('button', { name: 'Connect' }))
  const sessions = await screen.findByRole('list', { name: 'Sessions' })
  await vi.waitFor(() =>
    expect(within(sessions).getByRole('listitem')).toHaveTextContent('#1 web-01 CPU 23% · MEM 4.2/16G · / 61%'),
  )
  expect(asked('connect_host')).toEqual([{ alias: 'web-01' }])
  expect(status()).toHaveTextContent('Sessions 1')
  // The dashboard stays where it was.
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

test('Ctrl-K opens the palette, and so does the TUI leader Ctrl-A', async () => {
  const user = shell()
  await screen.findByRole('button', { name: /db-primary/ })
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  await user.keyboard('{Control>}k{/Control}')
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
  screen.getByRole<HTMLDialogElement>('dialog').close()
  await user.keyboard('{Control>}a{/Control}')
  expect(await screen.findByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
})

test('a host chosen in the palette is the selected one', async () => {
  const user = shell()
  await screen.findByRole('button', { name: /db-primary/ })
  await user.keyboard('{Control>}k{/Control}')
  await user.type(screen.getByRole('combobox'), 'db{Enter}')
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: /db-primary/ })).toHaveAttribute('aria-current', 'true')
  expect(status()).toHaveTextContent('db-primary postgres@db.internal.example.com')
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
  await user.type(screen.getByRole('combobox'), 'qc{Enter}')
  await user.type(screen.getByLabelText('Host'), '10.0.0.9')
  await user.type(screen.getByLabelText('User'), 'deploy')
  // Behind a modal dialog the cards and their buttons are inert; jsdom does not know.
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Connect' }))

  const sessions = await screen.findByRole('list', { name: 'Sessions' })
  expect(within(sessions).getByRole('listitem')).toHaveTextContent('#7 deploy@10.0.0.9')
  expect(sent).toEqual([{ host: 'deploy@10.0.0.9' }])
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(status()).toHaveTextContent('Sessions 1')
  expect(status()).toHaveTextContent('Hosts 4')
})

test('a refused quick connect says why, in the language of the window', async () => {
  const user = shell({ quick_connect: fails })
  await user.click(await screen.findByRole('button', { name: /Language/ }))
  await user.keyboard('{Control>}k{/Control}')
  await user.type(screen.getByRole('combobox'), 'qc{Enter}')
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
