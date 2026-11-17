import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { Forward } from './api'
import errors from './fixtures/errors.json'
import fixture from './fixtures/forwards.json'
import shown from './fixtures/shown.json'
import Forwards from './Forwards.svelte'
import { setLang } from './i18n.svelte'

type Args = Record<string, unknown>
const refused = errors.find((error) => error.class === 'refused')!
const taken = errors.find((error) => error.class === 'port_in_use')!
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])

/** The shell over web-01's three forwards; `over` replaces single commands. */
function shell(over: Record<string, (args: Args) => unknown> = {}) {
  let list = fixture as Forward[]
  const commands: Record<string, (args: Args) => unknown> = {
    forwards_list: () => list,
    forward_add: ({ kind, localPort, remote }) => {
      const [host, hport] = String(remote ?? ':0').split(':')
      const value = remote ? `${localPort} ${remote}` : String(localPort)
      const row = { kind, bind: '127.0.0.1', port: localPort, host, hport: Number(hport), value, state: 'off' } as Forward
      list = [...list, row]
      return row
    },
    // Starts what is off, stops what runs.
    forward_toggle: ({ kind, value }) => {
      const row = list.find((row) => row.kind === kind && row.value === value)!
      list = list.map((old) => (old === row ? { ...row, state: row.state === 'off' ? 'starting' : 'off' } : old))
      return row.state === 'off'
    },
    forward_kill: ({ kind, value }) => {
      list = list.filter((row) => row.kind !== kind || row.value !== value)
      return null
    },
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
  const onclose = vi.fn()
  render(Forwards, { host: 'web-01', onclose })
  return { onclose, user: userEvent.setup() }
}
const rows = () => screen.getAllByRole('row').slice(1)
const cells = () =>
  rows().map((row) => within(row).getAllByRole('cell').map((cell) => cell.textContent?.replace(/\s+/g, ' ').trim()))
const flip = (name: string) => screen.findByRole('switch', { name })
/** What `forward-state` says about a forward of a host. */
const says = (value: string, state: string, error: object | null = null, host = 'web-01') =>
  emit('forward-state', { host, kind: 'L', value, state, error })

afterEach(async () => {
  cleanup()
  await new Promise((done) => setTimeout(done))
  clearMocks()
  calls = []
})

test('a host\'s forwards: the kind of each, the port it listens on, where it leads, and whether it runs', async () => {
  shell()
  expect(screen.getByRole('dialog', { name: 'Forwards web-01' })).toBeInTheDocument()
  await flip('L 8080 localhost:80')
  expect(asked('forwards_list')).toEqual([{ host: 'web-01' }])
  expect(screen.getAllByRole('columnheader').map((th) => th.textContent?.trim())).toEqual([
    'Kind',
    'Local port',
    'Remote',
    'Status',
    'Delete',
  ])
  expect(cells()).toEqual([
    ['L', '8080', 'localhost:80', '○ Off', '×'],
    ['R', '9000', 'localhost:3000', '○ Off', '×'],
    // A SOCKS proxy leads wherever it is asked to.
    ['D', '1080', '—', '○ Off', '×'],
  ])
})

test('the switch starts a forward, and the window hears when it listens; again, and it is stopped', async () => {
  const { user } = shell()
  const web = await flip('L 8080 localhost:80')
  expect(web).toHaveAttribute('aria-checked', 'false')
  await user.click(web)
  expect(asked('forward_toggle')).toEqual([{ host: 'web-01', kind: 'L', value: '8080 localhost:80' }])
  await vi.waitFor(() => expect(web).toHaveTextContent('starting'))
  expect(web).toHaveAttribute('aria-checked', 'true')
  await says('8080 localhost:80', 'on')
  await vi.waitFor(() => expect(web).toHaveTextContent('● On'))
  // Another host's forward of the same line is not this one.
  await says('8080 localhost:80', 'off', null, 'web-02')
  expect(web).toHaveTextContent('● On')

  await user.click(web)
  await vi.waitFor(() => expect(web).toHaveTextContent('○ Off'))
  expect(web).toHaveAttribute('aria-checked', 'false')
  expect(asked('forward_toggle')).toHaveLength(2)
  expect(screen.queryByRole('alert')).toBeNull()
})

test('a forward that ends by itself is off again, and the kind of failure is said', async () => {
  const { user } = shell()
  const web = await flip('L 8080 localhost:80')
  await user.click(web)
  await says('8080 localhost:80', 'off', refused)
  await vi.waitFor(() => expect(web).toHaveTextContent('○ Off'))
  const alert = await screen.findByRole('alert')
  expect(alert).toHaveTextContent('✗ REFUSED Connection refused by 10.0.0.5:2222')
  expect(within(alert).getByText('✗ REFUSED')).toHaveClass('text-red')
  // No port is to blame: there is no other one to try.
  expect(screen.queryByRole('button', { name: /Try port/ })).toBeNull()
  setLang('vi')
  await vi.waitFor(() => expect(alert).toHaveTextContent('✗ BỊ TỪ CHỐI'))
})

test('Add: the kind, the port and where it leads; the new forward is in the table', async () => {
  const { user } = shell()
  await flip('L 8080 localhost:80')
  expect(screen.getByRole('radio', { name: 'L' })).toBeChecked()
  await user.type(screen.getByRole('spinbutton', { name: 'Local port' }), '8443')
  await user.type(screen.getByRole('textbox', { name: 'Remote' }), 'localhost:443')
  await user.click(screen.getByRole('button', { name: 'Add' }))
  expect(asked('forward_add')).toEqual([{ host: 'web-01', kind: 'L', localPort: 8443, remote: 'localhost:443' }])
  await vi.waitFor(() => expect(rows()).toHaveLength(4))
  expect(cells()[3]).toEqual(['L', '8443', 'localhost:443', '○ Off', '×'])
  // The form is ready for the next one.
  expect(screen.getByRole('spinbutton', { name: 'Local port' })).toHaveValue(null)

  // A dynamic forward has a port and nothing more.
  await user.click(screen.getByRole('radio', { name: 'D' }))
  expect(screen.getByRole('textbox', { name: 'Remote' })).toBeDisabled()
  await user.type(screen.getByRole('spinbutton', { name: 'Local port' }), '1081')
  await user.click(screen.getByRole('button', { name: 'Add' }))
  expect(asked('forward_add')[1]).toEqual({ host: 'web-01', kind: 'D', localPort: 1081, remote: null })
})

test('a port that is taken: the form says so, and offers the next one that is free', async () => {
  const { user } = shell({ forward_add: () => Promise.reject(taken) })
  await flip('L 8080 localhost:80')
  await user.type(screen.getByRole('spinbutton', { name: 'Local port' }), '8080')
  await user.type(screen.getByRole('textbox', { name: 'Remote' }), 'db:5432')
  await user.click(screen.getByRole('button', { name: 'Add' }))
  const alert = await screen.findByRole('alert')
  expect(alert).toHaveTextContent('✗ PORT IN USE Port 127.0.0.1:8080 is already in use')
  expect(rows()).toHaveLength(3)
  await user.click(screen.getByRole('button', { name: 'Try port 8081' }))
  expect(screen.getByRole('spinbutton', { name: 'Local port' })).toHaveValue(8081)
  // What was typed for the rest stays.
  expect(screen.getByRole('textbox', { name: 'Remote' })).toHaveValue('db:5432')
  expect(screen.queryByRole('alert')).toBeNull()
})

test('a port another program has: the switch goes back to off, and the form is filled for a port that is free', async () => {
  const { user } = shell()
  const web = await flip('L 8080 localhost:80')
  await user.click(web)
  // What the system said once the forward was tried.
  await says('8080 localhost:80', 'off', taken)
  await vi.waitFor(() => expect(web).toHaveTextContent('○ Off'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ PORT IN USE')
  await user.click(screen.getByRole('button', { name: 'Try port 8081' }))
  expect(screen.getByRole('spinbutton', { name: 'Local port' })).toHaveValue(8081)
  expect(screen.getByRole('textbox', { name: 'Remote' })).toHaveValue('localhost:80')
})

test('Delete takes a forward out of the table and out of the config', async () => {
  const { user } = shell()
  await flip('L 8080 localhost:80')
  await user.click(screen.getByRole('button', { name: 'Delete D 1080' }))
  expect(asked('forward_kill')).toEqual([{ host: 'web-01', kind: 'D', value: '1080' }])
  await vi.waitFor(() => expect(rows()).toHaveLength(2))
  expect(asked('forwards_list')).toHaveLength(2)
})

test('a host with no forwards says so, and the form is there', async () => {
  shell({ forwards_list: () => [] })
  expect(await screen.findByText('No forwards yet.')).toBeInTheDocument()
  expect(screen.queryByRole('table')).toBeNull()
  expect(screen.getByRole('button', { name: 'Add' })).toBeInTheDocument()
})

test('in Vietnamese: the TUI\'s words', async () => {
  setLang('vi')
  const { user } = shell()
  expect(screen.getByRole('dialog', { name: 'Chuyển tiếp web-01' })).toBeInTheDocument()
  const web = await flip('L 8080 localhost:80')
  expect(screen.getAllByRole('columnheader').map((th) => th.textContent?.trim())).toEqual([
    'Loại',
    'Cổng cục bộ',
    'Đích',
    'Trạng thái',
    'Xoá',
  ])
  expect(web).toHaveTextContent('○ Tắt')
  await user.click(web)
  await vi.waitFor(() => expect(web).toHaveTextContent('đang bật'))
  await says('8080 localhost:80', 'on')
  await vi.waitFor(() => expect(web).toHaveTextContent('● Bật'))
  expect(screen.getByRole('button', { name: 'Thêm' })).toBeInTheDocument()
})
