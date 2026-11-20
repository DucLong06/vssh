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
const meta = { grp: '', tags: '', notes: '', pinned: false, last_connected: 0, connects: 0, paused: false }
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
  jump_auth: [],
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
/** The road as its picture has it: this machine, the hops, the host. */
const way = () => within(screen.getByRole('list', { name: /The way there|Đường đi/ })).getAllByRole('listitem').map((li) => li.textContent?.replace('→', '').trim())
/** What is said under the rows of the road, of each hop. */
const said = () => [...screen.getByRole('list', { name: /Jump host|Máy trung gian/ }).querySelectorAll('li > p')].map((p) => p.textContent)
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
  expect(screen.getByRole('group', { name: 'Jump host' })).toHaveTextContent('Goes through these machines, in order.')
  expect(screen.queryByRole('list', { name: 'The way there' })).not.toBeInTheDocument()
  expect(screen.queryByLabelText('Key file')).not.toBeInTheDocument()

  await user.type(field('Alias'), 'db-02')
  await user.type(field('Host'), '10.0.0.9')
  await user.type(field('User'), 'postgres')
  await user.type(field('Port'), '5022')
  await user.click(radio('Key'))
  await user.type(field('Key file'), '~/.ssh/db')
  await user.click(button('Add a hop: web-01'))
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
        jump_auth: [],
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
  expect(button('Add a hop: web-02')).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Add a hop: web-01' })).toBeNull()

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

// --- D14: a file chosen in the desktop's dialog; the road through several jump hosts, each with what it is logged in to with

test('the folder button asks the desktop for a key file and writes its path into the field it stands by; closed without one, the field is as it was', async () => {
  let chosen: string | null = '~/.ssh/id_ed25519'
  const { user } = open({}, { pick_file: () => chosen })
  // No key, no button: the agent needs no file.
  expect(screen.queryByRole('button', { name: 'Choose a key file' })).not.toBeInTheDocument()
  await user.click(radio('Key'))
  await user.click(button('Choose a key file'))
  await vi.waitFor(() => expect(field('Key file')).toHaveValue('~/.ssh/id_ed25519'))
  // The dialog is the desktop's own: its title and the names of its two filters are all the window says of it.
  expect(asked('pick_file')).toEqual([{ title: 'Choose a key file', keys: ['SSH keys', 'All files'] }])

  // A hop's own button fills that hop's key, and no other field.
  await user.click(button('Add a hop: web-01'))
  await user.click(button('Add a hop: web-02'))
  chosen = '~/.ssh/hop'
  await user.click(button('Choose a key file · Hop 2'))
  await vi.waitFor(() => expect(field('Own key of hop 2')).toHaveValue('~/.ssh/hop'))
  expect(field('Own key of hop 1')).toHaveValue('')
  expect(field('Key file')).toHaveValue('~/.ssh/id_ed25519')

  // Closed with nothing chosen: what was there stays.
  chosen = null
  await user.click(button('Choose a key file'))
  await user.click(button('Choose a key file · Hop 2'))
  await vi.waitFor(() => expect(asked('pick_file')).toHaveLength(4))
  expect(field('Key file')).toHaveValue('~/.ssh/id_ed25519')
  expect(field('Own key of hop 2')).toHaveValue('~/.ssh/hop')

  await user.type(field('Alias'), 'db-02')
  await user.type(field('Host'), '10.0.0.9')
  await user.click(button('Save'))
  expect(asked('hosts_add')[0].form).toMatchObject({
    identity_file: '~/.ssh/id_ed25519',
    proxy_jump: 'web-01,web-02',
    jump_auth: [{ hop: 'web-02', key: '~/.ssh/hop', user: '' }],
  })
})

test('a dialog that cannot be opened says why in the form', async () => {
  const { user } = open({}, {
    pick_file: () => {
      throw other('no display')
    },
  })
  await user.click(radio('Key'))
  await user.click(button('Choose a key file'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR no display')
})

test('the road to a host: its hops in order, what each is logged in to with, and a key of its own before the jump host\'s', async () => {
  const forms: Record<string, Partial<Form>> = {
    'db-primary': { proxy_jump: 'web-01, ops@gw.example:2222,web-02', jump_auth: [{ hop: 'ops@gw.example:2222', key: '~/.ssh/gw', user: 'jump' }] },
    // What the jump hosts' own blocks say: a key for one, nothing for the other.
    'web-01': { identity_file: '~/.ssh/bastion' },
    'web-02': {},
  }
  const { user } = open({ alias: 'db-primary' }, { hosts_form: ({ alias }) => ({ ...web, alias, ...forms[alias as string] }) })
  await loaded()
  expect(way()).toEqual(['This machine', 'web-01', 'ops@gw.example:2222', 'web-02', 'db-primary'])
  expect(['Hop 1', 'Hop 2', 'Hop 3'].map((name) => (field(name) as HTMLInputElement).value)).toEqual(['web-01', 'ops@gw.example:2222', 'web-02'])
  expect(field('Own key of hop 2')).toHaveValue('~/.ssh/gw')
  expect(field('User of hop 2')).toHaveValue('jump')
  // A user goes with a key: where none is given there is none to type.
  expect(field('User of hop 1')).toBeDisabled()
  // Under each row: the jump host's own key, the key chosen here, the agent.
  await vi.waitFor(() =>
    expect(said()).toEqual([
      'Via web-01 — signs in with its own key, ~/.ssh/bastion',
      'Via ops@gw.example:2222 — signs in with the key chosen here, ~/.ssh/gw',
      'Via web-02 — signs in with the agent or the default keys',
    ]),
  )
  // Each jump host's block is read once, and the host's own.
  expect(asked('hosts_form').map((args) => args.alias).sort()).toEqual(['db-primary', 'web-01', 'web-02'])
  // The picture says the same of a hop that is rested on.
  const hops = within(screen.getByRole('list', { name: 'The way there' })).getAllByRole('listitem')
  expect(hops[1]).toHaveAttribute('title', 'Via web-01 — signs in with its own key, ~/.ssh/bastion')

  // A key chosen here comes before the jump host's own.
  await user.type(field('Own key of hop 1'), '~/.ssh/mine')
  expect(said()[0]).toBe('Via web-01 — signs in with the key chosen here, ~/.ssh/mine')
  expect(field('User of hop 1')).toBeEnabled()

  // Earlier, later, and off the road; the ends have nowhere further to go.
  expect(button('Move earlier · Hop 1')).toBeDisabled()
  expect(button('Move later · Hop 3')).toBeDisabled()
  await user.click(button('Move earlier · Hop 2'))
  expect(way()).toEqual(['This machine', 'ops@gw.example:2222', 'web-01', 'web-02', 'db-primary'])
  await user.click(button('Move later · Hop 2'))
  expect(way()).toEqual(['This machine', 'ops@gw.example:2222', 'web-02', 'web-01', 'db-primary'])
  await user.click(button('Delete · Hop 2'))
  expect(way()).toEqual(['This machine', 'ops@gw.example:2222', 'web-01', 'db-primary'])
  // What is off the road can be put on it again; what is on it is not offered twice.
  expect(button('Add a hop: web-02')).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Add a hop: web-01' })).toBeNull()

  // A hop that is typed: its row has the focus. Left empty it is no hop.
  await user.click(button('+ Add a hop'))
  expect(field('Hop 3')).toHaveFocus()
  expect(field('Hop 3')).toHaveAttribute('placeholder', 'alias or user@host[:port]')
  await user.click(button('Save'))
  // As ssh writes the road, and the keys beside it: a user only where there is a key.
  expect(asked('hosts_update')[0].form).toMatchObject({
    proxy_jump: 'ops@gw.example:2222,web-01',
    jump_auth: [
      { hop: 'ops@gw.example:2222', key: '~/.ssh/gw', user: 'jump' },
      { hop: 'web-01', key: '~/.ssh/mine', user: '' },
    ],
  })
})

test('a hop that is no host is said so, and nothing is sent: two words, the host itself', async () => {
  const { user } = open({ alias: 'web-01' })
  await loaded()
  await user.click(button('+ Add a hop'))
  for (const hop of ['two words', 'a@b@c', 'gw:port', 'web-01', 'me@web-01:22']) {
    await user.clear(field('Hop 1'))
    await user.type(field('Hop 1'), hop)
    await user.click(button('Save'))
    expect(field('Hop 1')).toBeInvalid()
    expect(screen.getByRole('group', { name: 'Jump host' })).toHaveAccessibleDescription(
      'A hop is an alias or user@host[:port], and not this host itself.',
    )
  }
  expect(asked('hosts_update')).toEqual([])
  // Typed in again it is not called wrong meanwhile; right, it goes.
  await user.clear(field('Hop 1'))
  await user.type(field('Hop 1'), 'ops@gw.example:2222')
  expect(field('Hop 1')).toBeValid()
  await user.click(button('Save'))
  expect(asked('hosts_update')[0].form).toMatchObject({ proxy_jump: 'ops@gw.example:2222', jump_auth: [] })
})

test('another ssh config is chosen in the desktop\'s dialog and previewed at once', async () => {
  const { user } = open({}, { pick_file: () => '~/work/ssh_config' })
  await user.click(button('Import from ssh config'))
  await screen.findAllByRole('checkbox')
  await user.click(button('Choose an ssh config'))
  await vi.waitFor(() => expect(field('Path')).toHaveValue('~/work/ssh_config'))
  // Any file: an ssh config has no name of its own to filter by.
  expect(asked('pick_file')).toEqual([{ title: 'Choose an ssh config' }])
  await vi.waitFor(() => expect(asked('hosts_import_preview').at(-1)).toEqual({ path: '~/work/ssh_config' }))
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
  expect(screen.getByRole('group', { name: 'Máy trung gian' })).toHaveTextContent('Đi qua các máy trung gian này, lần lượt.')
  await user.click(button('Thêm bước nhảy: web-01'))
  expect(way()).toEqual(['Máy bạn', 'web-01', 'Đích'])
  expect(screen.getByRole('group', { name: 'Máy trung gian' })).toHaveTextContent('Qua web-01 — đăng nhập bằng agent hoặc khoá mặc định')
  expect(field('Khoá riêng của bước nhảy 1')).toHaveAttribute('placeholder', 'Dùng khoá riêng cho bước nhảy')
  expect(field('Người dùng của bước nhảy 1')).toHaveAttribute('placeholder', 'người dùng (tuỳ chọn)')
  for (const name of ['Chọn tệp khoá · Bước nhảy 1', 'Chuyển lên trước · Bước nhảy 1', 'Chuyển xuống sau · Bước nhảy 1', 'Xoá · Bước nhảy 1']) {
    expect(button(name)).toBeInTheDocument()
  }
  await user.clear(field('Bước nhảy 1'))
  await user.type(field('Bước nhảy 1'), 'a b')
  await user.type(field('Cổng'), 'x')
  await user.click(button('Lưu'))
  expect(field('Tên gọi')).toHaveAccessibleDescription('Tên gọi phải là một từ, không có ký tự đại diện.')
  expect(field('Máy chủ')).toHaveAccessibleDescription('Chưa nhập máy chủ.')
  expect(field('Cổng')).toHaveAccessibleDescription('Cổng phải là số từ 1 đến 65535.')
  expect(screen.getByRole('group', { name: 'Máy trung gian' })).toHaveAccessibleDescription(
    'Bước nhảy là một tên gọi hoặc user@host[:port], và không phải chính máy này.',
  )
  await user.click(button('Nhập từ ssh config'))
  expect(await screen.findByRole('button', { name: 'Nhập 2' })).toBeInTheDocument()
  expect(screen.getByRole('dialog', { name: 'Nhập từ ssh config' })).toHaveTextContent('Máy chủ đã có được giữ nguyên.')
  expect(button('Xem trước')).toBeInTheDocument()
  expect(button('Chọn tệp ssh config')).toBeInTheDocument()
})

test('the widest dialog: the title and the buttons stay and the fields scroll between them, in two columns; a road of four hops is a numbered row a hop, and its picture cuts a long name short with the whole of it in a title', async () => {
  const { user } = open()
  const dialog = screen.getByRole('dialog')
  expect(dialog).toHaveClass('w-[min(78rem,94vw)]', 'mt-[6vh]', 'max-h-[88vh]', 'portrait:w-[94vw]', 'open:flex', 'flex-col')
  const form = dialog.querySelector('form')!
  expect(form).toHaveClass('flex', 'min-h-0', 'flex-col')
  const [, fields, foot] = [...form.children] as HTMLElement[]
  expect(fields).toHaveClass('min-h-0', 'flex-1', 'overflow-y-auto', 'landscape:grid-cols-[18rem_minmax(0,1fr)]')
  expect(within(foot).getByRole('button', { name: 'Save' })).toBeInTheDocument()
  expect(within(fields).queryByRole('button', { name: 'Save' })).toBeNull()
  // Who the host is on the left; the road and the rest on the right.
  const [left, right] = [...fields.children] as HTMLElement[]
  expect(fields.children).toHaveLength(2)
  expect(within(left).getAllByRole('textbox')).toHaveLength(4)
  expect(within(left).getByRole('radiogroup')).toBeInTheDocument()
  expect(within(right).getByRole('group', { name: 'Jump host' })).toBeInTheDocument()
  expect(within(right).getByRole('textbox', { name: 'Notes' })).toBeInTheDocument()

  const long = 'ops@a-bastion-with-a-name-nobody-reads-in-one-line.example.net'
  for (const name of ['web-01', 'web-02', 'db-primary']) await user.click(screen.getByRole('button', { name: `Add a hop: ${name}` }))
  await user.click(screen.getByRole('button', { name: '+ Add a hop' }))
  await user.keyboard(long)
  const rows = within(within(right).getByRole('list', { name: 'Jump host' })).getAllByRole('listitem')
  expect(rows).toHaveLength(4)
  rows.forEach((row, at) => {
    const n = at + 1
    expect(row).toHaveClass('flex', 'flex-wrap')
    expect(row.firstElementChild).toHaveTextContent(String(n))
    // As it is seen: the hop, the user, the key and its folder, the three buttons. As Tab goes: the key before the user.
    expect(within(row).getByRole('textbox', { name: `Hop ${n}` })).toHaveClass('order-1', 'min-w-64', 'flex-1', 'min-h-9')
    expect(within(row).getByRole('textbox', { name: `User of hop ${n}` })).toHaveClass('order-2', 'w-44!')
    expect(within(row).getByRole('textbox', { name: `Own key of hop ${n}` })).toHaveClass('order-3', 'flex-1')
    expect(within(row).getByRole('button', { name: `Choose a key file · Hop ${n}` })).toHaveClass('order-4', 'size-9')
    expect(within(row).getAllByRole('textbox').map((box) => box.getAttribute('aria-label'))).toEqual([`Hop ${n}`, `Own key of hop ${n}`, `User of hop ${n}`])
    for (const name of ['Move earlier', 'Move later', 'Delete']) expect(within(row).getByRole('button', { name: `${name} · Hop ${n}` })).toHaveClass('size-9')
    // What the hop is logged in to with, in words under it, and whole.
    expect(row.querySelector('p')).toHaveTextContent(/^Via .* signs in with/)
    expect(row.querySelector('p')).not.toHaveClass('truncate')
  })
  const picture = screen.getByRole('list', { name: 'The way there' })
  expect(picture).toHaveClass('flex', 'flex-wrap')
  expect(within(picture).getAllByRole('listitem')).toHaveLength(6)
  expect(within(picture).getByTitle(long)).toHaveClass('truncate', 'max-w-[14rem]')
  expect(within(picture).getByTitle(long)).toHaveTextContent(long)
})
