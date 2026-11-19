import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import HostsJson from './HostsJson.svelte'
import { setLang } from './i18n.svelte'
import { setStreamer } from './streamer.svelte'

type Args = Record<string, unknown>
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])
const other = (words: string) => ({ class: 'other', fail: null, chain: [words] })
/** A file of three hosts: the app has two of them. */
const offered = [
  { alias: 'web-01', hostname: '10.0.0.7', user: 'ops', port: 2200, taken: true },
  { alias: 'bare', hostname: '', user: '', port: null, taken: true },
  { alias: 'db', hostname: '10.0.0.9', user: '', port: null, taken: false },
]

function shell(mode: 'export' | 'import', over: Record<string, (args: Args) => unknown> = {}) {
  const commands: Record<string, (args: Args) => unknown> = {
    export_hosts_json: () => 4,
    import_hosts_json_preview: () => offered,
    import_hosts_json: ({ only, overwrite }) => (only as string[]).filter((alias) => overwrite || alias === 'db'),
    resolve_conn_error: ({ error }) => {
      const words = (error as ReturnType<typeof other>).chain[0]
      return { class: 'other', label: 'ERROR', message: words, chain: words }
    },
    ...over,
  }
  mockIPC((cmd, args) => {
    calls.push([cmd, args as Args])
    return commands[cmd](args as Args)
  })
  const heard = { onimported: vi.fn(), onclose: vi.fn() }
  render(HostsJson, { mode, ...heard })
  return { ...heard, user: userEvent.setup() }
}
const button = (name: string) => screen.getByRole('button', { name })
const rows = () => within(screen.getByRole('list', { name: 'Hosts' })).getAllByRole('listitem').map((row) => row.textContent?.replace(/\s+/g, ' ').trim())

afterEach(() => {
  cleanup()
  clearMocks()
  calls = []
})

test('export: the path is asked, the command gets it, and how many hosts went where is said', async () => {
  const { user, onclose } = shell('export')
  const dialog = screen.getByRole('dialog', { name: 'Export hosts (JSON)' })
  expect(dialog).toHaveTextContent('No password is written.')
  const path = screen.getByLabelText('Path')
  expect(path).toHaveFocus()
  expect(path).toHaveValue('~/vssh-hosts.json')
  await user.clear(path)
  await user.type(path, ' ~/out.json {Enter}')
  expect(asked('export_hosts_json')).toEqual([{ path: '~/out.json' }])
  expect(await screen.findByRole('status')).toHaveTextContent('4 hosts saved to ~/out.json')
  // Another path is another file: what was said of the last one goes.
  await user.type(path, 'x')
  expect(screen.queryByRole('status')).toBeNull()
  await user.click(button('Cancel'))
  expect(onclose).toHaveBeenCalledOnce()
})

test('export: a file that is there is not written over, and the dialog says so', async () => {
  const taken = () => {
    throw other('cannot write /home/me/vssh-hosts.json')
  }
  const { user } = shell('export', { export_hosts_json: taken })
  await user.click(button('Export'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR cannot write /home/me/vssh-hosts.json')
  expect(screen.queryByRole('status')).toBeNull()
})

test('import: nothing is written before the file is shown; hosts the app has are marked', async () => {
  const { user, onimported } = shell('import')
  expect(screen.getByRole('dialog', { name: 'Import hosts (JSON)' })).not.toHaveTextContent('No password')
  await user.click(button('Preview'))
  expect(asked('import_hosts_json_preview')).toEqual([{ path: '~/vssh-hosts.json' }])
  expect(rows()).toEqual(['web-01 ops@10.0.0.7:2200 already here', 'bare already here', 'db 10.0.0.9'])
  expect(asked('import_hosts_json')).toEqual([])
  expect(onimported).not.toHaveBeenCalled()
  expect(button('Import 3')).toBeEnabled()
})

test('import: hosts of the same name are asked about, and Skip leaves them as they are', async () => {
  const { user, onimported } = shell('import')
  await user.click(button('Preview'))
  await user.click(button('Import 3'))
  // Asked first: nothing is written by the first button.
  expect(asked('import_hosts_json')).toEqual([])
  expect(screen.getByRole('alert')).toHaveTextContent('2 of them are here already: web-01, bare. Overwrite them with the file\'s, or skip them?')
  await user.click(button('Skip'))
  expect(asked('import_hosts_json')).toEqual([{ path: '~/vssh-hosts.json', only: ['web-01', 'bare', 'db'], overwrite: false }])
  expect(onimported).toHaveBeenCalledExactlyOnceWith(['db'])
})

test('import: Overwrite takes the file\'s form for them, and only the hosts still ticked', async () => {
  const { user, onimported } = shell('import')
  await user.click(button('Preview'))
  await user.click(screen.getByRole('checkbox', { name: /bare/ }))
  await user.click(button('Import 2'))
  expect(screen.getByRole('alert')).toHaveTextContent('1 of them are here already: web-01.')
  await user.click(button('Overwrite'))
  expect(asked('import_hosts_json')).toEqual([{ path: '~/vssh-hosts.json', only: ['web-01', 'db'], overwrite: true }])
  expect(onimported).toHaveBeenCalledExactlyOnceWith(['web-01', 'db'])
})

test('import: with only new hosts ticked there is nothing to ask', async () => {
  const { user, onimported } = shell('import')
  await user.click(button('Preview'))
  await user.click(screen.getByRole('checkbox', { name: /web-01/ }))
  await user.click(screen.getByRole('checkbox', { name: /bare/ }))
  await user.click(button('Import 1'))
  expect(screen.queryByRole('alert')).toBeNull()
  expect(asked('import_hosts_json')).toEqual([{ path: '~/vssh-hosts.json', only: ['db'], overwrite: false }])
  expect(onimported).toHaveBeenCalledExactlyOnceWith(['db'])
})

test('import: a tick changed after the question takes the question back; with none ticked nothing can be imported', async () => {
  const { user } = shell('import')
  await user.click(button('Preview'))
  await user.click(button('Import 3'))
  await user.click(screen.getByRole('checkbox', { name: /db/ }))
  expect(screen.queryByRole('button', { name: 'Overwrite' })).toBeNull()
  await user.click(screen.getByRole('checkbox', { name: /web-01/ }))
  await user.click(screen.getByRole('checkbox', { name: /bare/ }))
  expect(button('Import 0')).toBeDisabled()
  expect(asked('import_hosts_json')).toEqual([])
})

test('import: a file that is no hosts file is said, an empty one too, and another path starts over', async () => {
  let answer: () => unknown = () => {
    throw other('/home/me/notes.json is not a hosts file')
  }
  const { user } = shell('import', { import_hosts_json_preview: () => answer() })
  await user.click(button('Preview'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR /home/me/notes.json is not a hosts file')
  expect(screen.queryByRole('button', { name: /^Import \d/ })).toBeNull()
  answer = () => []
  await user.click(button('Preview'))
  expect(await screen.findByText('No hosts in that file.')).toBeInTheDocument()
  expect(screen.queryByRole('alert')).toBeNull()
  answer = () => offered
  await user.click(button('Preview'))
  expect(rows()).toHaveLength(3)
  await user.type(screen.getByLabelText('Path'), '2')
  expect(screen.queryByRole('list', { name: 'Hosts' })).toBeNull()
})

test('in Vietnamese, and in streamer mode no address of the file is on screen', async () => {
  setLang('vi')
  setStreamer(true)
  const { user } = shell('import')
  expect(screen.getByRole('dialog', { name: 'Nhập máy chủ (JSON)' })).toBeInTheDocument()
  await user.click(button('Xem trước'))
  const list = screen.getByRole('list', { name: 'Máy chủ' })
  expect(list).toHaveTextContent('đã có')
  expect(list).not.toHaveTextContent('10.0.0.7')
  await user.click(button('Nhập 3'))
  expect(screen.getByRole('alert')).toHaveTextContent('2 máy trong số đó đã có: web-01, bare. Ghi đè bằng bản trong tệp, hay bỏ qua?')
  expect(button('Bỏ qua')).toBeInTheDocument()
  expect(button('Ghi đè')).toBeInTheDocument()
})

// --- D14: the file to read is chosen in the desktop's dialog

test('import: the folder button asks the desktop for the file and reads it at once; an export, which names a new file, has no such button', async () => {
  const { user } = shell('import', { pick_file: () => '~/Downloads/hosts.json' })
  await user.click(button('Choose a file'))
  await vi.waitFor(() => expect(screen.getByLabelText('Path')).toHaveValue('~/Downloads/hosts.json'))
  expect(asked('pick_file')).toEqual([{ title: 'Choose a file' }])
  await vi.waitFor(() => expect(asked('import_hosts_json_preview')).toEqual([{ path: '~/Downloads/hosts.json' }]))
  expect(await screen.findByRole('list', { name: 'Hosts' })).toBeInTheDocument()
  cleanup()

  shell('export')
  expect(screen.queryByRole('button', { name: 'Choose a file' })).not.toBeInTheDocument()
})
