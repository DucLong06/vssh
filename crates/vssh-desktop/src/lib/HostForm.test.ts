import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { Host, HostForm as Form, VaultState } from './api'
import fixture from './fixtures/hosts.json'
import { fromTarget } from './hosts'
import HostForm from './HostForm.svelte'
import { setLang } from './i18n.svelte'

type Args = Record<string, unknown>
const hosts: Host[] = fixture
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])
const other = (words: string) => ({ class: 'other', fail: null, chain: [words] })
const meta = { grp: '', tags: '', notes: '', pinned: false, last_connected: 0, connects: 0 }
/** What another ssh config has that the app has not. */
const theirs: Host[] = [
  { alias: 'bastion', hostname: '203.0.113.1', user: 'jump', port: null, meta },
  { alias: 'inner', hostname: '10.0.0.5', user: 'ops', port: 2200, meta },
]
/** web-01 as `hosts_form` reads it. */
const web: Form = {
  alias: 'web-01',
  hostname: '10.0.0.5',
  user: 'deploy',
  port: 2222,
  identity_file: '',
  proxy_jump: '',
  grp: 'prod',
  tags: 'web eu',
  notes: 'behind the balancer',
  pinned: true,
}

type Props = { alias?: string; start?: Partial<Form>; vault?: VaultState }
/** The form over the fixture's hosts; the vault has a password for web-01. `over` replaces single commands. */
function open(props: Props = {}, over: Record<string, (args: Args) => unknown> = {}) {
  const saved = ({ form }: Args) => ({ ...hosts[0], alias: (form as Form).alias })
  const commands: Record<string, (args: Args) => unknown> = {
    hosts_form: ({ alias }) => ({ ...web, alias }),
    hosts_add: saved,
    hosts_update: saved,
    vault_hosts: () => ['web-01'],
    hosts_import_preview: () => theirs,
    hosts_import_ssh_config: ({ only }) => only,
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
  const on = { onsaved: vi.fn(), ondelete: vi.fn(), onkeys: vi.fn(), onimported: vi.fn(), onclose: vi.fn() }
  render(HostForm, { hosts, vault: 'unlocked', ...props, ...on })
  return { ...on, user: userEvent.setup() }
}
const field = (name: string) => screen.getByLabelText(name)
const button = (name: string) => screen.getByRole('button', { name })
const radio = (name: string) => screen.getByRole('radio', { name })
const chips = () => within(screen.getByRole('list', { name: 'Tags' })).queryAllByRole('listitem').map((li) => li.firstElementChild?.textContent)
/** The form once the host's own values are in it. */
const loaded = () => vi.waitFor(() => expect(field('Host')).toHaveValue('10.0.0.5'))

afterEach(() => {
  cleanup()
  clearMocks()
  calls = []
})

test('a new host: every field goes to the shell the way it reads the form', async () => {
  const { user, onsaved, onclose } = open()
  expect(screen.getByRole('dialog', { name: 'Add host' })).toBeInTheDocument()
  expect(field('Alias')).toHaveFocus()
  // Keys of the agent, no jump host, not pinned: what `ssh` does when nothing is said.
  expect(radio('Agent')).toBeChecked()
  expect(within(screen.getByRole('radiogroup', { name: 'Jump host' })).getByRole('radio', { name: 'none' })).toBeChecked()
  expect(screen.queryByLabelText('Key file')).not.toBeInTheDocument()

  await user.type(field('Alias'), 'db-02')
  await user.type(field('Host'), '10.0.0.9')
  await user.type(field('User'), 'postgres')
  await user.type(field('Port'), '5022')
  await user.click(radio('Key'))
  await user.type(field('Key file'), '~/.ssh/db')
  await user.click(radio('web-01'))
  // Enter, a space or a comma ends a tag; none of them sends the form.
  await user.type(field('add a tag'), 'db{Enter}eu,replica ')
  expect(chips()).toEqual(['db', 'eu', 'replica'])
  await user.click(button('Delete replica'))
  expect(asked('hosts_add')).toEqual([])
  await user.type(field('Group'), 'prod')
  await user.type(field('Notes'), 'failover of db-primary')
  await user.click(screen.getByRole('checkbox', { name: 'pinned' }))
  await user.click(button('Save'))

  expect(asked('hosts_add')).toEqual([
    {
      form: {
        alias: 'db-02',
        hostname: '10.0.0.9',
        user: 'postgres',
        port: 5022,
        identity_file: '~/.ssh/db',
        proxy_jump: 'web-01',
        grp: 'prod',
        tags: 'db eu',
        notes: 'failover of db-primary',
        pinned: true,
      },
    },
  ])
  await vi.waitFor(() => expect(onsaved).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ alias: 'db-02' })))
  expect(onclose).not.toHaveBeenCalled()
})

test('what is wrong is said under its field, and nothing is sent until it is right', async () => {
  const { user, onsaved } = open()
  await user.click(button('Save'))
  expect(field('Alias')).toBeInvalid()
  expect(field('Alias')).toHaveAccessibleDescription('The alias must be one word without wildcards.')
  expect(field('Host')).toBeInvalid()
  expect(field('Host')).toHaveAccessibleDescription('The host must not be empty.')
  // No port is port 22.
  expect(field('Port')).toBeValid()
  // The first of them has the focus.
  expect(field('Alias')).toHaveFocus()

  // What is being put right is no longer called wrong; the other field still is.
  await user.type(field('Alias'), 'x')
  expect(field('Alias')).toBeValid()
  expect(field('Alias')).toHaveAccessibleDescription('')
  expect(field('Host')).toBeInvalid()
  expect(screen.getAllByRole('alert')).toHaveLength(1)

  for (const alias of ['two words', 'web-*', 'a=b']) {
    await user.clear(field('Alias'))
    await user.type(field('Alias'), alias)
    await user.click(button('Save'))
    expect(field('Alias')).toHaveAccessibleDescription('The alias must be one word without wildcards.')
  }
  await user.clear(field('Alias'))
  await user.type(field('Alias'), 'web-01')
  await user.click(button('Save'))
  expect(field('Alias')).toHaveAccessibleDescription('web-01 already exists')

  await user.type(field('Host'), '10.0.0.9')
  for (const port of ['ssh', '0', '65536', '22.5', '-1']) {
    await user.clear(field('Port'))
    await user.type(field('Port'), port)
    await user.click(button('Save'))
    expect(field('Port')).toBeInvalid()
    expect(field('Port')).toHaveAccessibleDescription('The port must be a number from 1 to 65535.')
  }
  expect(field('Host')).toBeValid()
  expect(screen.getAllByRole('alert')).toHaveLength(2)
  expect(asked('hosts_add')).toEqual([])

  await user.clear(field('Alias'))
  await user.type(field('Alias'), 'web-03')
  await user.clear(field('Port'))
  await user.type(field('Port'), '65535')
  await user.click(button('Save'))
  expect(asked('hosts_add')).toHaveLength(1)
  expect(asked('hosts_add')[0].form).toMatchObject({ alias: 'web-03', hostname: '10.0.0.9', port: 65535 })
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  await vi.waitFor(() => expect(onsaved).toHaveBeenCalledOnce())
})

test('a host is edited from what its own block says; its stored password is left alone unless one is typed', async () => {
  const { user, onsaved } = open({ alias: 'web-01' })
  expect(screen.getByRole('dialog', { name: 'Edit host' })).toBeInTheDocument()
  await loaded()
  expect(asked('hosts_form')).toEqual([{ alias: 'web-01' }])
  expect(field('Alias')).toHaveValue('web-01')
  expect(field('User')).toHaveValue('deploy')
  expect(field('Port')).toHaveValue('2222')
  expect(chips()).toEqual(['web', 'eu'])
  expect(field('Group')).toHaveValue('prod')
  expect(field('Notes')).toHaveValue('behind the balancer')
  expect(screen.getByRole('checkbox', { name: 'pinned' })).toBeChecked()
  // The vault has its password: that is how it is signed in to. What it is, the window never knows.
  expect(radio('Vault password')).toBeChecked()
  expect(field('Password')).toHaveValue('')
  expect(field('Password')).toHaveAttribute('type', 'password')
  expect(field('Password')).toHaveAttribute('placeholder', 'password stored')
  // Itself is no jump host of its own.
  expect(within(screen.getByRole('radiogroup', { name: 'Jump host' })).queryByRole('radio', { name: 'web-01' })).toBeNull()

  // Its own alias is not taken.
  await user.click(button('Delete eu'))
  await user.click(button('Save'))
  expect(asked('hosts_update')).toEqual([{ alias: 'web-01', form: { ...web, tags: 'web' } }])
  await vi.waitFor(() => expect(onsaved).toHaveBeenCalledOnce())

  await user.type(field('Password'), 'hunter3')
  await user.click(button('Save'))
  expect(asked('hosts_update')[1].form).toEqual({ ...web, tags: 'web', password: 'hunter3' })
})

test('a host that is given a key or left to the agent has its stored password taken out', async () => {
  const { user } = open({ alias: 'web-01' })
  await loaded()
  await user.click(radio('Agent'))
  expect(screen.queryByLabelText('Password')).not.toBeInTheDocument()
  await user.click(button('Save'))
  expect(asked('hosts_update')[0].form).toEqual({ ...web, password: '' })
})

test('a key in the host\'s block is how it is signed in to; emptied, that is the agent again', async () => {
  const { user } = open({ alias: 'web-02' }, { hosts_form: () => ({ ...web, alias: 'web-02', identity_file: '~/.ssh/web' }) })
  await loaded()
  expect(radio('Key')).toBeChecked()
  expect(field('Key file')).toHaveValue('~/.ssh/web')
  await user.click(radio('Agent'))
  await user.click(button('Save'))
  // It has no password in the vault: nothing is said of one.
  expect(asked('hosts_update')[0].form).toEqual({ ...web, alias: 'web-02', identity_file: '' })
})

test('while the vault is locked no password can be typed, and none is sent', async () => {
  const { user } = open({ vault: 'locked' })
  await user.click(radio('Vault password'))
  expect(field('Password')).toBeDisabled()
  expect(field('Password')).toHaveAccessibleDescription('Unlock the vault to store a password.')
  await user.type(field('Alias'), 'db-02')
  await user.type(field('Host'), '10.0.0.9')
  await user.click(button('Save'))
  expect(asked('hosts_add')[0].form).not.toHaveProperty('password')
  // The vault is not asked what it holds while it is shut.
  expect(asked('vault_hosts')).toEqual([])
})

test('a quick connect is saved through the form, which opens with what was typed for it', async () => {
  expect(fromTarget('root@example.com:2200')).toEqual({ alias: 'example.com', hostname: 'example.com', user: 'root', port: 2200 })
  expect(fromTarget('example.com')).toEqual({ alias: 'example.com', hostname: 'example.com', user: '', port: null })
  expect(fromTarget('root@[::1]:2200')).toEqual({ alias: '::1', hostname: '::1', user: 'root', port: 2200 })
  expect(fromTarget('::1')).toEqual({ alias: '::1', hostname: '::1', user: '', port: null })

  const { user } = open({ start: fromTarget('root@example.com:2200') })
  expect(screen.getByRole('dialog', { name: 'Add host' })).toBeInTheDocument()
  expect(field('Alias')).toHaveValue('example.com')
  expect(field('Host')).toHaveValue('example.com')
  expect(field('User')).toHaveValue('root')
  expect(field('Port')).toHaveValue('2200')
  await user.click(button('Save'))
  expect(asked('hosts_add')[0].form).toMatchObject({ alias: 'example.com', hostname: 'example.com', user: 'root', port: 2200 })
})

test('the shell\'s refusal is said in the form, which stays open', async () => {
  const { user, onsaved } = open({ alias: 'web-01' }, {
    hosts_update: () => {
      throw other('the vault is locked')
    },
  })
  await loaded()
  await user.click(button('Save'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR the vault is locked')
  expect(onsaved).not.toHaveBeenCalled()
  expect(button('Save')).toBeEnabled()
})

test('a host that is there can be deleted, and one with a password given a key; a new one neither', async () => {
  const { user, ondelete, onkeys } = open({ alias: 'web-01' })
  await loaded()
  await user.click(button('Delete'))
  expect(ondelete).toHaveBeenCalledExactlyOnceWith('web-01')
  await user.click(button('Set up SSH key'))
  expect(onkeys).toHaveBeenCalledExactlyOnceWith('web-01')
  expect(screen.queryByRole('button', { name: 'Import from ssh config' })).not.toBeInTheDocument()
  cleanup()

  // No password in the vault: nothing to replace by a key.
  open({ alias: 'web-02' })
  await loaded()
  expect(button('Delete')).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Set up SSH key' })).not.toBeInTheDocument()
  cleanup()

  open()
  expect(screen.queryByRole('button', { name: 'Delete' })).not.toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Set up SSH key' })).not.toBeInTheDocument()
})

test('an import shows what it would add, every host ticked; only those left ticked are taken', async () => {
  const { user, onimported } = open()
  await user.click(button('Import from ssh config'))
  expect(screen.getByRole('dialog', { name: 'Import from ssh config' })).toBeInTheDocument()
  // `~/.ssh/config` unless another file is named.
  expect(field('Path')).toHaveAttribute('placeholder', '~/.ssh/config')
  const rows = await screen.findAllByRole('checkbox')
  expect(asked('hosts_import_preview')).toEqual([{ path: null }])
  expect(rows.map((row) => row.closest('label')?.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
    'bastion jump@203.0.113.1',
    'inner ops@10.0.0.5:2200',
  ])
  expect(rows.every((row) => (row as HTMLInputElement).checked)).toBe(true)
  expect(screen.getByRole('dialog')).toHaveTextContent('Hosts you already have are left as they are.')
  expect(button('Import 2')).toBeEnabled()

  await user.click(screen.getByRole('checkbox', { name: /bastion/ }))
  await user.click(button('Import 1'))
  expect(asked('hosts_import_ssh_config')).toEqual([{ path: null, only: ['inner'] }])
  await vi.waitFor(() => expect(onimported).toHaveBeenCalledExactlyOnceWith(['inner']))
})

test('another file is previewed by its path; one with nothing new says so, and one that cannot be read says why', async () => {
  let found: Host[] | Error = []
  const { user, onimported } = open({}, {
    hosts_import_preview: () => {
      if (found instanceof Error) throw other(found.message)
      return found
    },
  })
  await user.click(button('Import from ssh config'))
  expect(await screen.findByText('No new hosts in that file.')).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: /^Import \d/ })).not.toBeInTheDocument()

  found = theirs
  await user.type(field('Path'), '~/work/ssh_config')
  await user.click(button('Preview'))
  expect(await screen.findAllByRole('checkbox')).toHaveLength(2)
  expect(asked('hosts_import_preview').at(-1)).toEqual({ path: '~/work/ssh_config' })
  expect(screen.queryByText('No new hosts in that file.')).not.toBeInTheDocument()
  // Nothing ticked: nothing to import.
  for (const row of screen.getAllByRole('checkbox')) await user.click(row)
  expect(button('Import 0')).toBeDisabled()

  found = new Error('cannot read /home/me/nowhere')
  await user.clear(field('Path'))
  await user.type(field('Path'), '~/nowhere{Enter}')
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR cannot read /home/me/nowhere')
  expect(screen.queryByRole('checkbox')).not.toBeInTheDocument()

  // Back to the form, with nothing imported.
  await user.click(button('Cancel'))
  expect(screen.getByRole('dialog', { name: 'Add host' })).toBeInTheDocument()
  expect(onimported).not.toHaveBeenCalled()
})

test('cancel closes it and nothing is saved', async () => {
  const { user, onclose } = open()
  await user.type(field('Alias'), 'db-02')
  await user.click(button('Cancel'))
  expect(onclose).toHaveBeenCalledOnce()
  expect(asked('hosts_add')).toEqual([])
})

test('in Vietnamese: the form, what is wrong in it, and the import', async () => {
  setLang('vi')
  const { user } = open()
  expect(screen.getByRole('dialog', { name: 'Thêm máy chủ' })).toBeInTheDocument()
  for (const name of ['Tên gọi', 'Máy chủ', 'Người dùng', 'Cổng', 'thêm thẻ', 'Nhóm', 'Ghi chú']) {
    expect(field(name)).toBeInTheDocument()
  }
  expect(screen.getByRole('radiogroup', { name: 'Đăng nhập bằng' })).toHaveTextContent('Mật khẩu trong kho Khoá Agent')
  expect(screen.getByRole('radiogroup', { name: 'Máy trung gian' })).toHaveTextContent('không')
  await user.type(field('Cổng'), 'x')
  await user.click(button('Lưu'))
  expect(field('Tên gọi')).toHaveAccessibleDescription('Tên gọi phải là một từ, không có ký tự đại diện.')
  expect(field('Máy chủ')).toHaveAccessibleDescription('Chưa nhập máy chủ.')
  expect(field('Cổng')).toHaveAccessibleDescription('Cổng phải là số từ 1 đến 65535.')
  await user.click(button('Nhập từ ssh config'))
  expect(await screen.findByRole('button', { name: 'Nhập 2' })).toBeInTheDocument()
  expect(screen.getByRole('dialog', { name: 'Nhập từ ssh config' })).toHaveTextContent('Máy chủ đã có được giữ nguyên.')
  expect(button('Xem trước')).toBeInTheDocument()
})
