import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import listing from '../../src-tauri/tests/fixtures/files.json'
import type { Entry } from './api'
import errors from './fixtures/errors.json'
import shown from './fixtures/shown.json'
import Files from './Files.svelte'
import { setLang } from './i18n.svelte'

type Args = Record<string, unknown>
const timeout = errors.find((error) => error.class === 'timeout')!
const mine: Entry[] = [
  { name: 'Documents', is_dir: true, size: 4096, mtime: 1763300000 },
  { name: 'auth.log', is_dir: false, size: 12, mtime: 1763300000 },
  { name: 'notes.txt', is_dir: false, size: 2048, mtime: 1763300000 },
]
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])

/** The shell, answering like the real one; `over` replaces single commands. */
function shell(over: Record<string, (args: Args) => unknown> = {}) {
  const commands: Record<string, (args: Args) => unknown> = {
    local_list: ({ path }) => ({ path: path || '/home/me', entries: mine }),
    sftp_realpath: () => '/home/deploy',
    sftp_list: () => listing,
    local_mkdir: () => null,
    local_rm: () => null,
    local_rename: () => null,
    sftp_mkdir: () => null,
    sftp_rm: () => null,
    sftp_rename: () => null,
    // A transfer answers when it is over: these never are, unless a test ends them.
    sftp_upload: () => new Promise(() => {}),
    sftp_download: () => new Promise(() => {}),
    sftp_cancel: () => true,
    resolve_conn_error: ({ error, lang }) =>
      shown[lang as 'en' | 'vi'][errors.findIndex((known) => known.class === (error as typeof timeout).class)],
    ...over,
  }
  mockIPC(
    (cmd, args) => {
      calls.push([cmd, args as Args])
      return commands[cmd](args as Args)
    },
    { shouldMockEvents: true },
  )
  render(Files, { host: 'web-01', active: true })
  return userEvent.setup()
}
const grid = (name: string) => screen.getByRole('grid', { name })
const row = (side: string, name: string) => within(grid(side)).findByRole('row', { name: new RegExp(name) })
const names = (side: string) =>
  within(grid(side))
    .getAllByRole('row')
    .slice(1)
    .map((row) => row.dataset.name)
const path = (side: string) =>
  within(within(screen.getByRole('region', { name: side })).getByRole('navigation'))
    .getAllByRole('button')
    .map((crumb) => crumb.textContent?.trim())
    .join(' ')

afterEach(async () => {
  vi.useRealTimers()
  cleanup()
  await new Promise((done) => setTimeout(done))
  clearMocks()
  calls = []
})

test('two panes: this machine\'s home on the left, the directory the host starts in on the right', async () => {
  shell()
  await row('local', 'Documents')
  await row('web-01', 'nginx')
  expect(names('local')).toEqual(['Documents', 'auth.log', 'notes.txt'])
  expect(names('web-01')).toEqual(['nginx', 'auth.log', 'empty'])
  expect(path('local')).toBe('/ home me')
  expect(path('web-01')).toBe('/ home deploy')
  expect(asked('local_list')).toEqual([{ path: '' }])
  expect(asked('sftp_realpath')).toEqual([{ host: 'web-01', path: '' }])
  expect(asked('sftp_list')).toEqual([{ host: 'web-01', path: '/home/deploy' }])
  // The keys go to the local pane first.
  expect(grid('local')).toHaveFocus()
  expect(screen.getByRole('button', { name: 'Upload' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Download' })).toBeInTheDocument()
})

test('going into a directory lists it, on the side it is on', async () => {
  const user = shell()
  await user.dblClick(await row('web-01', 'nginx'))
  await vi.waitFor(() => expect(path('web-01')).toBe('/ home deploy nginx'))
  expect(asked('sftp_list').at(-1)).toEqual({ host: 'web-01', path: '/home/deploy/nginx' })
  // Where the host starts was asked once.
  expect(asked('sftp_realpath')).toHaveLength(1)
  await row('local', 'Documents')
  grid('local').focus()
  await user.keyboard('{Enter}')
  await vi.waitFor(() => expect(path('local')).toBe('/ home me Documents'))
  expect(asked('local_list').at(-1)).toEqual({ path: '/home/me/Documents' })
})

test('a host that is dead: its pane says which kind of failure, this machine\'s works, and it can be tried again', async () => {
  let dead = true
  const user = shell({
    sftp_realpath: () => {
      if (dead) throw timeout
      return '/home/deploy'
    },
  })
  const pane = screen.getByRole('region', { name: 'web-01' })
  await vi.waitFor(() => expect(within(pane).getByRole('alert')).toHaveTextContent('✗ TIMEOUT'))
  expect(within(pane).getByRole('alert')).toHaveTextContent('10.0.0.5:2222')
  expect(names('local')).toHaveLength(3)

  // In the language of the window.
  setLang('vi')
  const vi_words = shown.vi[errors.indexOf(timeout)]
  await vi.waitFor(() => expect(within(pane).getByRole('alert')).toHaveTextContent(vi_words.message))
  expect(screen.getByRole('grid', { name: 'máy mình' })).toBeInTheDocument()
  setLang('en')

  dead = false
  await user.click(await within(pane).findByRole('button', { name: 'try again' }))
  await row('web-01', 'nginx')
  expect(within(pane).queryByRole('alert')).toBeNull()
})

test('a directory that cannot be read: the pane says so and stays where it was', async () => {
  const user = shell({
    sftp_list: ({ path }) => {
      if (path !== '/home/deploy') throw { class: 'other', fail: null, chain: ['Permission denied'] }
      return listing
    },
    resolve_conn_error: () => ({ class: 'other', label: 'ERROR', message: 'Permission denied', chain: 'Permission denied' }),
  })
  await user.dblClick(await row('web-01', 'nginx'))
  const pane = screen.getByRole('region', { name: 'web-01' })
  await vi.waitFor(() => expect(within(pane).getByRole('alert')).toHaveTextContent('✗ ERROR Permission denied'))
  expect(path('web-01')).toBe('/ home deploy')
  await user.click(within(pane).getByRole('button', { name: 'try again' }))
  await row('web-01', 'nginx')
})
