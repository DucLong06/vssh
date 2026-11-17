import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte'
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
  vi.restoreAllMocks()
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

// --- M2: copies, the menu, and how far a transfer is ---

const transfers = () => within(screen.getByRole('list', { name: 'Transfers' })).getAllByRole('listitem')
const noTransfers = () => expect(screen.queryByRole('list', { name: 'Transfers' })).toBeNull()
/** Both panes filled, and the keys in the pane asked for. */
async function ready(user: ReturnType<typeof shell>, side = 'local') {
  await screen.findByRole('row', { name: /Documents/ })
  await screen.findByRole('row', { name: /nginx/ })
  grid(side).focus()
  return user
}
/** A transfer that ends when the test says so. */
function held() {
  const waiting: Record<string, { done: () => void; fail: (why: unknown) => void }> = {}
  const start = ({ id }: Args) =>
    new Promise<null>((done, fail) => (waiting[id as string] = { done: () => done(null), fail }))
  return { waiting, start }
}
const menu = () => screen.getByRole('menu')
const items = () => within(menu()).getAllByRole('menuitem').map((item) => item.textContent?.trim())
const rightClick = (user: ReturnType<typeof userEvent.setup>, target: Element) =>
  user.pointer({ keys: '[MouseRight]', target })
const dialog = () => screen.getByRole('dialog')

test('c copies the selected file to the directory the other pane shows: up from here, down from there', async () => {
  const user = await ready(shell())
  await user.keyboard('jjc')
  expect(asked('sftp_upload')).toHaveLength(1)
  const { id, ...upload } = asked('sftp_upload')[0]
  expect(upload).toEqual({ host: 'web-01', local: '/home/me/notes.txt', remote: '/home/deploy/notes.txt' })
  expect(id).toMatch(/^[A-Za-z0-9_-]+$/)
  expect(transfers()).toHaveLength(1)
  expect(within(transfers()[0]).getByRole('img', { name: 'Upload' })).toBeInTheDocument()
  expect(within(transfers()[0]).getByRole('progressbar', { name: 'notes.txt' })).toHaveAttribute('aria-valuenow', '0')

  grid('web-01').focus()
  await user.keyboard('Gc')
  const { id: other, ...download } = asked('sftp_download')[0]
  expect(download).toEqual({ host: 'web-01', remote: '/home/deploy/empty', local: '/home/me/empty' })
  expect(other).not.toBe(id)
  expect(transfers()).toHaveLength(2)
  expect(within(transfers()[1]).getByRole('img', { name: 'Download' })).toBeInTheDocument()
})

test('entries marked in both panes: one c copies each to the other side, and the marks are gone', async () => {
  const user = await ready(shell())
  // notes.txt here, empty there.
  await user.keyboard('jj ')
  grid('web-01').focus()
  await user.keyboard('G c')
  expect(asked('sftp_upload').map((args) => args.local)).toEqual(['/home/me/notes.txt'])
  expect(asked('sftp_download').map((args) => args.remote)).toEqual(['/home/deploy/empty'])
  expect(screen.queryAllByRole('img', { name: 'selected' })).toHaveLength(0)
})

test('the buttons between the panes copy what is marked on their side, or what is selected there', async () => {
  const user = await ready(shell())
  await user.keyboard('jj')
  await user.click(screen.getByRole('button', { name: 'Upload' }))
  expect(asked('sftp_upload').map((args) => args.remote)).toEqual(['/home/deploy/notes.txt'])
  await user.click(await row('web-01', 'empty'))
  await user.click(screen.getByRole('button', { name: 'Download' }))
  expect(asked('sftp_download').map((args) => args.local)).toEqual(['/home/me/empty'])
})

test('a directory is not copied, and the view says so', async () => {
  const user = await ready(shell())
  await user.keyboard('c')
  expect(screen.getByRole('status')).toHaveTextContent('Only files can be copied.')
  expect(asked('sftp_upload')).toHaveLength(0)
  noTransfers()
})

test('a file that is there already is not overwritten without a yes', async () => {
  const user = await ready(shell())
  // auth.log is on both sides.
  await user.keyboard('jc')
  expect(dialog()).toHaveTextContent('auth.log exists on the other side. Overwrite?')
  expect(asked('sftp_upload')).toHaveLength(0)
  await user.click(within(dialog()).getByRole('button', { name: 'Cancel' }))
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(asked('sftp_upload')).toHaveLength(0)

  // A webview gives the focus back to where it was when a dialog closes; jsdom does not.
  grid('local').focus()
  await user.keyboard('c')
  await user.click(within(dialog()).getByRole('button', { name: 'Overwrite' }))
  expect(asked('sftp_upload').map((args) => args.remote)).toEqual(['/home/deploy/auth.log'])
})

test('a transfer says how far it is: bar, per cent, speed and the time left; done, it goes and the pane is read again', async () => {
  const now = vi.spyOn(Date, 'now').mockReturnValue(1_000_000)
  const { waiting, start } = held()
  const user = await ready(shell({ sftp_upload: start }))
  await user.keyboard('jjc')
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(1))
  const id = asked('sftp_upload')[0].id as string

  // Two seconds on, a quarter is through.
  now.mockReturnValue(1_002_000)
  await emit(`sftp-progress:${id}`, { done: 2 * 1024 * 1024, total: 8 * 1024 * 1024 })
  const [line] = transfers()
  await vi.waitFor(() => expect(within(line).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '25'))
  expect(line).toHaveTextContent('notes.txt')
  expect(line).toHaveTextContent('25%')
  expect(line).toHaveTextContent('1.0 M/s')
  expect(line).toHaveTextContent('6s')

  const lists = asked('sftp_list').length
  waiting[id].done()
  await vi.waitFor(noTransfers)
  // What arrived is in the pane it went to.
  await vi.waitFor(() => expect(asked('sftp_list')).toHaveLength(lists + 1))
  expect(asked('sftp_list').at(-1)).toEqual({ host: 'web-01', path: '/home/deploy' })
})

test('Cancel stops a transfer where it is, and its line goes without a complaint', async () => {
  const { waiting, start } = held()
  const user = await ready(shell({ sftp_upload: start }))
  await user.keyboard('jjc')
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(1))
  const id = asked('sftp_upload')[0].id as string
  await user.click(screen.getByRole('button', { name: 'Cancel notes.txt' }))
  expect(asked('sftp_cancel')).toEqual([{ id }])
  // The shell's answer to a cancelled transfer.
  waiting[id].fail({ class: 'other', fail: null, chain: ['transfer cancelled'] })
  await vi.waitFor(noTransfers)
  expect(screen.queryByRole('alert')).toBeNull()
})

test('a transfer that fails keeps its line, with the kind of failure, until it is closed', async () => {
  const { waiting, start } = held()
  const user = await ready(shell({ sftp_upload: start }))
  await user.keyboard('jjc')
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(1))
  waiting[asked('sftp_upload')[0].id as string].fail(timeout)
  await vi.waitFor(() => expect(within(transfers()[0]).getByRole('alert')).toHaveTextContent('✗ TIMEOUT'))
  expect(within(transfers()[0]).queryByRole('progressbar')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'close notes.txt' }))
  noTransfers()
  expect(asked('sftp_cancel')).toHaveLength(0)
})

test('several transfers: a line each, three at a time, the rest waiting their turn', async () => {
  const many: Entry[] = ['a', 'b', 'c', 'd', 'e'].map((name) => ({ name, is_dir: false, size: 1, mtime: 0 }))
  const { waiting, start } = held()
  const user = shell({ local_list: () => ({ path: '/home/me', entries: many }), sftp_upload: start })
  await row('local', 'a')
  await row('web-01', 'nginx')
  grid('local').focus()
  await user.keyboard('     c')
  expect(transfers()).toHaveLength(5)
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(3))
  expect(transfers()[3]).toHaveTextContent('waiting')
  expect(transfers()[4]).toHaveTextContent('waiting')
  // One that waits can be taken off the list before it starts.
  await user.click(screen.getByRole('button', { name: 'Cancel e' }))
  expect(transfers()).toHaveLength(4)
  expect(asked('sftp_cancel')).toHaveLength(0)

  waiting[asked('sftp_upload')[0].id as string].done()
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(4))
  expect(asked('sftp_upload').map((args) => args.remote)).toEqual(
    ['a', 'b', 'c', 'd'].map((name) => `/home/deploy/${name}`),
  )
  await vi.waitFor(() => expect(transfers()).toHaveLength(3))
})

test('the right button opens a menu: a new folder, and for an entry a copy, another name, or its removal', async () => {
  const user = await ready(shell())
  await rightClick(user, await row('web-01', 'nginx'))
  // A directory is not copied.
  expect(items()).toEqual(['New folder', 'Rename', 'Delete'])
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('menu')).toBeNull()
  await rightClick(user, await row('web-01', 'auth.log'))
  expect(items()).toEqual(['Copy', 'New folder', 'Rename', 'Delete'])
  // Under the pointer's empty space: only what needs no entry.
  await rightClick(user, screen.getByRole('region', { name: 'local' }))
  expect(items()).toEqual(['New folder'])
  // A click anywhere else closes it.
  await user.click(grid('local'))
  expect(screen.queryByRole('menu')).toBeNull()

  await rightClick(user, await row('web-01', 'empty'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Copy' }))
  expect(asked('sftp_download').map((args) => args.local)).toEqual(['/home/me/empty'])
})

test('New folder asks for a name and makes it, on the side the menu was opened on', async () => {
  const user = await ready(shell())
  await rightClick(user, await row('web-01', 'nginx'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'New folder' }))
  const name = within(dialog()).getByRole('textbox', { name: 'Name' })
  expect(name).toHaveFocus()
  // Nothing to make yet; and not over what is there.
  expect(within(dialog()).getByRole('button', { name: 'New folder' })).toBeDisabled()
  await user.keyboard('nginx')
  expect(dialog()).toHaveTextContent('nginx already exists')
  expect(within(dialog()).getByRole('button', { name: 'New folder' })).toBeDisabled()
  await user.clear(name)
  const lists = asked('sftp_list').length
  await user.keyboard('releases{Enter}')
  expect(asked('sftp_mkdir')).toEqual([{ host: 'web-01', path: '/home/deploy/releases' }])
  expect(screen.queryByRole('dialog')).toBeNull()
  await vi.waitFor(() => expect(asked('sftp_list')).toHaveLength(lists + 1))

  await rightClick(user, await row('local', 'notes.txt'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'New folder' }))
  await user.keyboard('work{Enter}')
  expect(asked('local_mkdir')).toEqual([{ path: '/home/me/work' }])
  expect(asked('local_list').at(-1)).toEqual({ path: '/home/me' })
})

test('Rename starts from the name there is; Delete asks first', async () => {
  const user = await ready(shell())
  await rightClick(user, await row('web-01', 'auth.log'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Rename' }))
  const name = within(dialog()).getByRole('textbox', { name: 'Name' })
  expect(name).toHaveValue('auth.log')
  expect(name).toHaveFocus()
  await user.keyboard('auth.old{Enter}')
  expect(asked('sftp_rename')).toEqual([
    { host: 'web-01', from: '/home/deploy/auth.log', to: '/home/deploy/auth.old' },
  ])

  await rightClick(user, await row('web-01', 'nginx'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Delete' }))
  expect(dialog()).toHaveTextContent('Delete nginx?')
  expect(asked('sftp_rm')).toHaveLength(0)
  await user.click(within(dialog()).getByRole('button', { name: 'Delete' }))
  expect(asked('sftp_rm')).toEqual([{ host: 'web-01', path: '/home/deploy/nginx', isDir: true }])

  await rightClick(user, await row('local', 'notes.txt'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Delete' }))
  await user.click(within(dialog()).getByRole('button', { name: 'Cancel' }))
  expect(asked('local_rm')).toHaveLength(0)
  await rightClick(user, await row('local', 'notes.txt'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Delete' }))
  await user.click(within(dialog()).getByRole('button', { name: 'Delete' }))
  expect(asked('local_rm')).toEqual([{ path: '/home/me/notes.txt', isDir: false }])
  await rightClick(user, await row('local', 'notes.txt'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Rename' }))
  // The name is selected: what is typed takes its place.
  await user.keyboard('notes.bak{Enter}')
  expect(asked('local_rename')).toEqual([{ from: '/home/me/notes.txt', to: '/home/me/notes.bak' }])
})

test('a change the host refuses is said, and can be put away', async () => {
  const refuses = () => {
    throw { class: 'other', fail: null, chain: ['Directory not empty'] }
  }
  const words = { class: 'other', label: 'ERROR', message: 'Directory not empty', chain: 'Directory not empty' }
  const user = await ready(shell({ sftp_rm: refuses, resolve_conn_error: () => words }))
  await rightClick(user, await row('web-01', 'nginx'))
  await user.click(within(menu()).getByRole('menuitem', { name: 'Delete' }))
  await user.click(within(dialog()).getByRole('button', { name: 'Delete' }))
  await vi.waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('✗ ERROR Directory not empty'))
  await user.click(screen.getByRole('button', { name: 'close' }))
  expect(screen.queryByRole('alert')).toBeNull()
})

test('a row dragged onto the other pane is copied there; marked rows go together', async () => {
  const user = await ready(shell())
  await fireEvent.dragStart(await row('local', 'notes.txt'))
  // Onto its own pane: nothing.
  await fireEvent.drop(screen.getByRole('region', { name: 'local' }))
  expect(asked('sftp_upload')).toHaveLength(0)
  await fireEvent.dragStart(await row('local', 'notes.txt'))
  await fireEvent.drop(screen.getByRole('region', { name: 'web-01' }))
  await vi.waitFor(() => expect(asked('sftp_upload').map((args) => args.remote)).toEqual(['/home/deploy/notes.txt']))

  // The other way, with two rows marked: a drag of one of them takes both.
  grid('web-01').focus()
  await user.keyboard('j  ')
  await fireEvent.dragStart(await row('web-01', 'empty'))
  await fireEvent.drop(screen.getByRole('region', { name: 'local' }))
  // auth.log is here already: both wait for the yes.
  await user.click(within(dialog()).getByRole('button', { name: 'Overwrite' }))
  await vi.waitFor(() =>
    expect(asked('sftp_download').map((args) => args.local)).toEqual(['/home/me/auth.log', '/home/me/empty']),
  )
})

test('files dropped on the window from the desktop go up to the directory the host\'s pane shows', async () => {
  await ready(shell())
  await emit('tauri://drag-drop', { paths: ['/tmp/photos/a.jpg', '/tmp/b.bin'], position: { x: 700, y: 300 } })
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(2))
  expect(asked('sftp_upload').map(({ local, remote }) => [local, remote])).toEqual([
    ['/tmp/photos/a.jpg', '/home/deploy/a.jpg'],
    ['/tmp/b.bin', '/home/deploy/b.bin'],
  ])
  // A drag inside the window says the same, without files.
  await emit('tauri://drag-drop', { paths: [], position: { x: 700, y: 300 } })
  expect(asked('sftp_upload')).toHaveLength(2)
})

test('behind another tab the view takes no drop; closed, it stops what it was moving', async () => {
  const { start } = held()
  const user = shell({ sftp_upload: start })
  const view = render(Files, { host: 'web-02', active: false })
  await ready(user)
  await emit('tauri://drag-drop', { paths: ['/tmp/b.bin'], position: { x: 0, y: 0 } })
  await vi.waitFor(() => expect(asked('sftp_upload')).toHaveLength(1))
  expect(asked('sftp_upload')[0].host).toBe('web-01')
  view.unmount()
  expect(asked('sftp_cancel')).toHaveLength(0)
  cleanup()
  expect(asked('sftp_cancel')).toEqual([{ id: asked('sftp_upload')[0].id }])
})

test('the view in Vietnamese: Tải lên, Tải xuống, Sao chép, Huỷ', async () => {
  setLang('vi')
  const user = await ready(shell({ sftp_upload: held().start }), 'máy mình')
  expect(screen.getByRole('button', { name: 'Tải lên' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Tải xuống' })).toBeInTheDocument()
  await user.keyboard('jjc')
  expect(within(screen.getByRole('list', { name: 'Truyền tệp' })).getByRole('button', { name: 'Huỷ notes.txt' })).toHaveTextContent('Huỷ')
  await rightClick(user, await row('máy mình', 'notes.txt'))
  expect(items()).toEqual(['Sao chép', 'Tạo thư mục', 'Đổi tên', 'Xoá'])
  await user.click(within(menu()).getByRole('menuitem', { name: 'Xoá' }))
  expect(dialog()).toHaveTextContent('Xoá notes.txt?')
})
