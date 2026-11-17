import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { Snippet } from './api'
import hosts from './fixtures/hosts.json'
import { setLang } from './i18n.svelte'
import Snippets from './Snippets.svelte'

type Args = Record<string, unknown>
const restart = 'systemctl restart {{unit}} && journalctl -u {{ unit }} -n {{lines}} -o "{{.Go}}"'
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])

/** The shell with three snippets in its database; `over` replaces single commands. */
function shell(props: { picked?: string[]; snippets?: Snippet[] } = {}, over: Record<string, (args: Args) => unknown> = {}) {
  let list: Snippet[] = [
    { id: 1, host: '', name: 'up', command: 'uptime' },
    { id: 2, host: '', name: 'restart', command: restart },
    { id: 3, host: 'web-01', name: 'logs', command: 'tail -n 20 /var/log/nginx/error.log' },
  ]
  const commands: Record<string, (args: Args) => unknown> = {
    list_snippets: () => list,
    snippets_create: (args) => {
      const made = { id: 9, ...args } as Snippet
      list = [...list, made]
      return made
    },
    snippets_update: (args) => {
      list = list.map((old) => (old.id === args.id ? (args as Snippet) : old))
      return args
    },
    snippets_delete: ({ id }) => {
      list = list.filter((old) => old.id !== id)
      return null
    },
    // Not a failure of a connection: the shell's own words, as they are.
    resolve_conn_error: ({ error }) => {
      const words = (error as { chain: string[] }).chain[0]
      return { class: 'other', label: 'ERROR', message: words, chain: words }
    },
    ...over,
  }
  mockIPC((cmd, args) => {
    calls.push([cmd, args as Args])
    return commands[cmd](args as Args)
  })
  const heard = { onrun: vi.fn(), onclose: vi.fn() }
  render(Snippets, { snippets: list, hosts, ...heard, ...props })
  return { ...heard, user: userEvent.setup() }
}
const rows = () => within(screen.getByRole('list')).getAllByRole('listitem')
const button = (name: string) => screen.getByRole('button', { name })
const field = (name: string) => screen.getByRole('textbox', { name })

afterEach(() => {
  cleanup()
  clearMocks()
  calls = []
})

test('the list: every snippet with its name, the host it is for, and its command', () => {
  shell()
  expect(screen.getByRole('dialog', { name: 'Snippets' })).toBeInTheDocument()
  expect(rows().map((row) => row.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
    'up all hosts uptime Run Edit Delete',
    `restart all hosts ${restart} Run Edit Delete`,
    'logs web-01 tail -n 20 /var/log/nginx/error.log Run Edit Delete',
  ])
})

test('opened, it reads the snippets again: one made in the TUI meanwhile is there', async () => {
  // What the window had when it started: nothing.
  shell({ snippets: [] })
  await vi.waitFor(() => expect(rows()).toHaveLength(3))
  expect(asked('list_snippets')).toHaveLength(1)
})

test('the keys follow what the dialog shows: the first field of a form, Cancel under a question', async () => {
  const { user } = shell()
  await user.click(button('Run restart'))
  await vi.waitFor(() => expect(field('unit')).toHaveFocus())
  // Enter in the last field runs it: no reaching for the pointer.
  await user.keyboard('nginx{Tab}50{Enter}')
  await user.click(button('Cancel'))
  await user.click(button('Delete up'))
  await vi.waitFor(() => expect(button('Cancel')).toHaveFocus())
  await user.click(button('Cancel'))
  await user.click(button('Edit up'))
  await vi.waitFor(() => expect(field('Name')).toHaveFocus())
})

test('no snippets: the list says so, and Add is there', () => {
  shell({ snippets: [] }, { list_snippets: () => [] })
  expect(screen.getByText('No snippets yet.')).toBeInTheDocument()
  expect(button('Add')).toBeInTheDocument()
})

test('Add: a form for the name, the command and the host it is for; saved, it is in the list', async () => {
  const { user } = shell()
  await user.click(button('Add'))
  await user.type(field('Name'), 'disk')
  await user.type(field('Command'), 'df -h /')
  // For every host unless one is chosen.
  expect(screen.getByRole('radio', { name: 'all hosts' })).toBeChecked()
  await user.click(screen.getByRole('radio', { name: 'db-primary' }))
  await user.click(button('Save'))
  expect(asked('snippets_create')).toEqual([{ name: 'disk', command: 'df -h /', host: 'db-primary' }])
  await vi.waitFor(() => expect(rows()).toHaveLength(4))
  expect(rows()[3]).toHaveTextContent('disk db-primary df -h /')
})

test('Edit: the form with what the snippet has; saved under the id it had', async () => {
  const { user } = shell()
  await user.click(button('Edit logs'))
  expect(field('Name')).toHaveValue('logs')
  expect(field('Command')).toHaveValue('tail -n 20 /var/log/nginx/error.log')
  expect(screen.getByRole('radio', { name: 'web-01' })).toBeChecked()
  await user.clear(field('Name'))
  await user.type(field('Name'), 'errors')
  await user.click(screen.getByRole('radio', { name: 'all hosts' }))
  await user.click(button('Save'))
  expect(asked('snippets_update')).toEqual([
    { id: 3, name: 'errors', command: 'tail -n 20 /var/log/nginx/error.log', host: '' },
  ])
  await vi.waitFor(() => expect(rows()[2]).toHaveTextContent('errors all hosts'))
  expect(asked('snippets_create')).toHaveLength(0)
})

test('a snippet the shell will not save: the form stays and says why', async () => {
  const refusal = { class: 'other', fail: null, chain: ['no host named "gone"'] }
  const { user } = shell({}, { snippets_create: () => Promise.reject(refusal) })
  await user.click(button('Add'))
  await user.type(field('Name'), 'disk')
  await user.type(field('Command'), 'df -h /')
  await user.click(button('Save'))
  expect(await screen.findByRole('alert')).toHaveTextContent('no host named "gone"')
  expect(field('Name')).toHaveValue('disk')
  // Cancel goes back to the list, which is as it was.
  await user.click(button('Cancel'))
  expect(rows()).toHaveLength(3)
})

test('Delete asks first; after a yes the snippet is gone', async () => {
  const { user } = shell()
  await user.click(button('Delete up'))
  expect(screen.getByText('Delete up?')).toBeInTheDocument()
  expect(asked('snippets_delete')).toHaveLength(0)
  await user.click(button('Cancel'))
  expect(rows()).toHaveLength(3)
  await user.click(button('Delete up'))
  await user.click(button('Delete'))
  expect(asked('snippets_delete')).toEqual([{ id: 1 }])
  await vi.waitFor(() => expect(rows()).toHaveLength(2))
})

test('Run on a snippet with parameters asks for each, once; the hosts chosen on the dashboard are ticked', async () => {
  const { user, onrun } = shell({ picked: ['db-primary'] })
  await user.click(button('Run restart'))
  // `{{ unit }}` is `{{unit}}` again, and `{{.Go}}` is no parameter.
  const fields = screen.getAllByRole('textbox')
  expect(fields.map((input) => input.getAttribute('aria-label'))).toEqual(['unit', 'lines'])
  expect(fields[0]).toHaveAttribute('placeholder', 'value for unit')
  expect(screen.getByRole('group', { name: 'Run on 1 host(s)' })).toBeInTheDocument()
  expect(screen.getByRole('checkbox', { name: 'db-primary' })).toBeChecked()
  expect(screen.getByRole('checkbox', { name: 'web-01' })).not.toBeChecked()
  await user.type(field('unit'), 'nginx')
  await user.type(field('lines'), '50')
  await user.click(screen.getByRole('checkbox', { name: 'web-01' }))
  expect(screen.getByRole('group', { name: 'Run on 2 host(s)' })).toBeInTheDocument()
  await user.click(button('Run'))
  // The hosts in the order they have on the dashboard.
  expect(onrun).toHaveBeenCalledWith(['web-01', 'db-primary'], restart, { unit: 'nginx', lines: '50' })
})

test('a snippet for one host is run there; with no host ticked there is nothing to run', async () => {
  const { user, onrun } = shell({ picked: ['web-02'] })
  await user.click(button('Run logs'))
  // Not on the hosts chosen on the dashboard: it is not theirs.
  expect(screen.getAllByRole('checkbox')).toHaveLength(1)
  expect(screen.getByRole('checkbox', { name: 'web-01' })).toBeChecked()
  expect(screen.queryByRole('textbox')).toBeNull()
  await user.click(screen.getByRole('checkbox', { name: 'web-01' }))
  expect(button('Run')).toBeDisabled()
  await user.click(screen.getByRole('checkbox', { name: 'web-01' }))
  await user.click(button('Run'))
  expect(onrun).toHaveBeenCalledWith(['web-01'], 'tail -n 20 /var/log/nginx/error.log', {})
})

test('in Vietnamese: the TUI\'s words', async () => {
  setLang('vi')
  const { user } = shell({ picked: ['web-01', 'web-02'] })
  expect(screen.getByRole('dialog', { name: 'Đoạn lệnh' })).toBeInTheDocument()
  expect(rows()[0]).toHaveTextContent('up mọi máy chủ uptime Chạy Sửa Xoá')
  await user.click(button('Chạy restart'))
  expect(screen.getByRole('group', { name: 'Chạy trên 2 máy' })).toBeInTheDocument()
  expect(field('unit')).toHaveAttribute('placeholder', 'giá trị cho unit')
  await user.click(button('Huỷ'))
  await user.click(button('Thêm'))
  expect(field('Lệnh')).toBeInTheDocument()
  expect(button('Lưu')).toBeInTheDocument()
})
