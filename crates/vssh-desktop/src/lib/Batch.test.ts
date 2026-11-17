import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { Host } from './api'
import Batch from './Batch.svelte'
import errors from './fixtures/errors.json'
import fixture from './fixtures/hosts.json'
import shown from './fixtures/shown.json'
import { setLang } from './i18n.svelte'

type Args = Record<string, unknown>
const hosts: Host[] = fixture.slice(0, 3)
const refused = errors.find((error) => error.class === 'refused')!
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])

/** The shell: batches are numbered from 7. `over` replaces single commands. */
function shell(props: { command?: string; params?: Record<string, string> } = {}, over: Record<string, (args: Args) => unknown> = {}) {
  let batches = 6
  const commands: Record<string, (args: Args) => unknown> = {
    snippet_run: () => ++batches,
    'plugin:clipboard-manager|write_text': () => null,
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
  render(Batch, { hosts, onclose, ...props })
  return { onclose, user: userEvent.setup() }
}
/** A host's answer to a batch, as `batch-progress` carries it. */
const answers = (id: number, host: string, output: string | null, error: object | null = null) =>
  emit('batch-progress', { id, host, output, error })
const row = (host: string) =>
  within(screen.getByRole('list', { name: 'Hosts' }))
    .getAllByRole('listitem')
    .find((item) => item.dataset.host === host)!
const spinner = (host: string) => within(row(host)).queryByRole('img', { name: 'running' })

afterEach(async () => {
  cleanup()
  await new Promise((done) => setTimeout(done))
  clearMocks()
  calls = []
})

test('a snippet\'s command is run at once on its hosts: a spinner each, then what each one printed', async () => {
  shell({ command: 'systemctl restart {{unit}}', params: { unit: 'nginx' } })
  expect(screen.getByRole('dialog', { name: 'Run on 3 host(s)' })).toBeInTheDocument()
  await vi.waitFor(() => expect(asked('snippet_run')).toHaveLength(1))
  expect(asked('snippet_run')[0]).toEqual({
    hosts: ['web-01', 'web-02', 'db-primary'],
    command: 'systemctl restart {{unit}}',
    params: { unit: 'nginx' },
  })
  await vi.waitFor(() => expect(spinner('web-01')).toBeInTheDocument())
  expect(spinner('web-02')).toBeInTheDocument()
  expect(screen.getByRole('dialog')).toHaveTextContent('0/3')

  await answers(7, 'web-02', ' 15:02:11 up 12 days\n')
  await vi.waitFor(() => expect(row('web-02')).toHaveTextContent('15:02:11 up 12 days'))
  expect(spinner('web-02')).toBeNull()
  // The others are still at it.
  expect(spinner('web-01')).toBeInTheDocument()
  expect(screen.getByRole('dialog')).toHaveTextContent('1/3')
  // A command that prints nothing has answered all the same.
  await answers(7, 'web-01', '')
  await vi.waitFor(() => expect(row('web-01')).toHaveTextContent('(no output)'))
  expect(spinner('web-01')).toBeNull()
})

test('a host that fails says which kind of failure, in red on its own line: nobody is skipped', async () => {
  shell({ command: 'uptime' })
  await vi.waitFor(() => expect(asked('snippet_run')).toHaveLength(1))
  await answers(7, 'web-01', 'ok\n')
  await answers(7, 'web-02', null, refused)
  const alert = await within(row('web-02')).findByRole('alert')
  await vi.waitFor(() => expect(alert).toHaveTextContent('✗ REFUSED Connection refused by 10.0.0.5:2222'))
  expect(within(alert).getByText('✗ REFUSED')).toHaveClass('text-red')
  expect(spinner('web-02')).toBeNull()
  expect(within(row('web-01')).queryByRole('alert')).toBeNull()
  expect(screen.getByRole('dialog')).toHaveTextContent('2/3')
  // In the language of the window.
  setLang('vi')
  await vi.waitFor(() => expect(alert).toHaveTextContent('✗ BỊ TỪ CHỐI 10.0.0.5:2222 từ chối kết nối'))
  expect(screen.getByRole('dialog', { name: 'Chạy trên 3 máy' })).toBeInTheDocument()
})

test('an answer is heard even before the batch has its number; another batch\'s answers are not this one\'s', async () => {
  let number: (id: number) => void = () => {}
  shell({ command: 'uptime' }, { snippet_run: () => new Promise((done) => (number = done)) })
  await vi.waitFor(() => expect(asked('snippet_run')).toHaveLength(1))
  // A host that is quick to refuse answers while the command's own answer is on its way.
  await answers(7, 'web-01', 'early\n')
  await answers(3, 'web-02', 'somebody else\'s\n')
  number(7)
  await vi.waitFor(() => expect(row('web-01')).toHaveTextContent('early'))
  expect(row('web-02')).not.toHaveTextContent('somebody else')
  expect(spinner('web-02')).toBeInTheDocument()
})

test('a batch the shell will not run says why', async () => {
  const refusal = { class: 'other', fail: null, chain: ['no value for {{unit}}'] }
  shell({ command: 'restart {{unit}}' }, {
    snippet_run: () => Promise.reject(refusal),
    resolve_conn_error: () => ({ class: 'other', label: 'ERROR', message: refusal.chain[0], chain: refusal.chain[0] }),
  })
  expect(await screen.findByRole('alert')).toHaveTextContent('no value for {{unit}}')
  expect(spinner('web-01')).toBeNull()
})
