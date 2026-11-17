import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import { setLang } from './i18n.svelte'
import Vault from './Vault.svelte'

type Args = Record<string, unknown>
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])
/** What the shell rejects with for a failure that is not the network's. */
const other = (words: string) => ({ class: 'other', fail: null, chain: [words] })

/** The shell over a vault whose passphrase is `master`; `over` replaces single commands. */
function shell(state: 'none' | 'locked' | 'unlocked', over: Record<string, (args: Args) => unknown> = {}, once = false) {
  let master = 'master'
  const open = (given: unknown) => {
    if (given !== master) throw other('wrong passphrase or damaged vault')
  }
  const commands: Record<string, (args: Args) => unknown> = {
    vault_unlock: ({ passphrase }) => (state === 'none' ? null : open(passphrase)),
    vault_hosts: () => ['web-01', 'db-primary'],
    vault_pass_change: ({ old, new: next }) => {
      open(old)
      master = String(next)
    },
    vault_export: () => null,
    local_list: () => ({ path: '/home/me', entries: [] }),
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
  const onclose = vi.fn()
  render(Vault, { vault: state, hosts: 4, once, onclose })
  return { onclose, user: userEvent.setup() }
}
const field = (name: string) => screen.getByLabelText(name)
const button = (name: string) => screen.getByRole('button', { name })

afterEach(() => {
  cleanup()
  clearMocks()
  calls = []
})

test('a locked vault asks for its passphrase; the right one opens it for the manager', async () => {
  const { user } = shell('locked')
  const dialog = screen.getByRole('dialog', { name: 'Vault' })
  expect(dialog).toHaveTextContent('Vault locked')
  expect(field('Vault passphrase')).toHaveAttribute('type', 'password')
  expect(field('Vault passphrase')).toHaveFocus()
  // What it holds is not said while it is locked.
  expect(dialog).not.toHaveTextContent('Passwords')
  expect(asked('vault_hosts')).toEqual([])

  await user.type(field('Vault passphrase'), 'master{Enter}')
  expect(asked('vault_unlock')).toEqual([{ passphrase: 'master' }])
  expect(await screen.findByText('Change passphrase')).toBeInTheDocument()
  expect(dialog).toHaveTextContent('Vault unlocked')
  expect(dialog).toHaveTextContent('Hosts 4')
  expect(dialog).toHaveTextContent('Passwords 2')
})

test('a wrong passphrase says so and asks again; the third time warns, and locks nobody out', async () => {
  const { user, onclose } = shell('locked')
  for (const guess of ['mastre', 'mister']) {
    await user.type(field('Vault passphrase'), `${guess}{Enter}`)
    expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR Wrong vault passphrase, or the vault file is damaged')
    // The core's own words are kept, out of the way.
    expect(screen.getByRole('alert')).toHaveAttribute('title', 'wrong passphrase or damaged vault')
    // The guess is gone, and the field is there for the next one.
    expect(field('Vault passphrase')).toHaveValue('')
    expect(field('Vault passphrase')).toHaveFocus()
  }
  expect(screen.queryByRole('status')).not.toBeInTheDocument()

  await user.type(field('Vault passphrase'), 'muster{Enter}')
  expect(await screen.findByRole('status')).toHaveTextContent(
    '3 wrong attempts. The vault does not lock you out, but a forgotten passphrase cannot be recovered.',
  )
  expect(button('Unlock')).toBeEnabled()
  expect(screen.getByRole('dialog')).toHaveTextContent('Vault locked')

  // A fourth try is as good as the first.
  await user.type(field('Vault passphrase'), 'master{Enter}')
  expect(await screen.findByText('Change passphrase')).toBeInTheDocument()
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
  expect(asked('vault_unlock')).toHaveLength(4)
  expect(onclose).not.toHaveBeenCalled()
})

test('where there is no vault the first passphrase makes it, asked for twice', async () => {
  const { user } = shell('none')
  expect(screen.getByRole('dialog')).toHaveTextContent('There is no vault yet.')
  await user.type(field('Vault passphrase'), 'battery staple')
  await user.type(field('Repeat passphrase'), 'battery stable')
  await user.click(button('Create vault'))
  expect(await screen.findByRole('alert')).toHaveTextContent('passphrases do not match')
  expect(field('Repeat passphrase')).toBeInvalid()
  expect(asked('vault_unlock')).toEqual([])

  await user.clear(field('Repeat passphrase'))
  await user.type(field('Repeat passphrase'), 'battery staple')
  await user.click(button('Create vault'))
  expect(asked('vault_unlock')).toEqual([{ passphrase: 'battery staple' }])
  expect(await screen.findByText('Change passphrase')).toBeInTheDocument()
})

test('the passphrase is changed by who knows the old one and types the new one twice the same', async () => {
  const { user } = shell('unlocked')
  expect(await screen.findByText('Passwords')).toBeInTheDocument()
  const change = button('Change passphrase')
  for (const name of ['Current passphrase', 'New passphrase', 'Repeat passphrase']) {
    expect(field(name)).toBeRequired()
    expect(field(name)).toHaveAttribute('type', 'password')
  }

  await user.type(field('Current passphrase'), 'master')
  await user.type(field('New passphrase'), 'battery staple')
  await user.type(field('Repeat passphrase'), 'battery stable')
  await user.click(change)
  expect(await screen.findByRole('alert')).toHaveTextContent('passphrases do not match')
  expect(field('Repeat passphrase')).toBeInvalid()
  expect(asked('vault_pass_change')).toEqual([])

  // The old one wrong: the shell's refusal, and nothing is changed.
  await user.clear(field('Current passphrase'))
  await user.type(field('Current passphrase'), 'guess')
  await user.clear(field('Repeat passphrase'))
  await user.type(field('Repeat passphrase'), 'battery staple')
  await user.click(change)
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR Wrong vault passphrase, or the vault file is damaged')
  expect(field('Repeat passphrase')).toBeValid()

  await user.clear(field('Current passphrase'))
  await user.type(field('Current passphrase'), 'master')
  await user.click(change)
  expect(await screen.findByRole('status')).toHaveTextContent('Passphrase changed.')
  expect(asked('vault_pass_change').at(-1)).toEqual({ old: 'master', new: 'battery staple' })
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  // Nothing of it stays in the window.
  for (const name of ['Current passphrase', 'New passphrase', 'Repeat passphrase']) expect(field(name)).toHaveValue('')
})

test('a backup is exported to a file in the home directory, or wherever the path says', async () => {
  const { user } = shell('unlocked')
  await vi.waitFor(() => expect(field('Path')).toHaveValue('/home/me/vssh-vault.backup'))
  expect(screen.getByRole('dialog')).toHaveTextContent('An encrypted copy, opened by the same passphrase.')
  await user.click(button('Export backup'))
  expect(asked('vault_export')).toEqual([{ path: '/home/me/vssh-vault.backup' }])
  expect(await screen.findByRole('status')).toHaveTextContent('Saved to /home/me/vssh-vault.backup')

  await user.clear(field('Path'))
  await user.type(field('Path'), '/mnt/usb/vault.copy')
  await user.click(button('Export backup'))
  expect(asked('vault_export').at(-1)).toEqual({ path: '/mnt/usb/vault.copy' })
})

test('an export that is refused says why', async () => {
  const { user } = shell('unlocked', {
    vault_export: () => {
      throw other('/home/me/vssh-vault.backup already exists')
    },
  })
  await vi.waitFor(() => expect(field('Path')).toHaveValue('/home/me/vssh-vault.backup'))
  await user.click(button('Export backup'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR /home/me/vssh-vault.backup already exists')
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
})

test('opened only to ask, it closes once it is unlocked; a wrong passphrase keeps it open', async () => {
  const { user, onclose } = shell('locked', {}, true)
  await user.type(field('Vault passphrase'), 'mastre{Enter}')
  expect(await screen.findByRole('alert')).toBeInTheDocument()
  expect(onclose).not.toHaveBeenCalled()
  await user.type(field('Vault passphrase'), 'master{Enter}')
  await vi.waitFor(() => expect(onclose).toHaveBeenCalledOnce())
})

test('unlocked for the manager, the focus is in its first field, not lost with the form that went', async () => {
  const { user } = shell('locked')
  await user.type(field('Vault passphrase'), 'master{Enter}')
  await vi.waitFor(() => expect(field('Current passphrase')).toHaveFocus())
})

test('cancel closes it, and it stays locked', async () => {
  const { user, onclose } = shell('locked')
  await user.click(button('Cancel'))
  expect(onclose).toHaveBeenCalledOnce()
  expect(asked('vault_unlock')).toEqual([])
})

test('in Vietnamese it is the TUI\'s kho', async () => {
  setLang('vi')
  const { user } = shell('locked')
  expect(screen.getByRole('dialog', { name: 'Kho' })).toHaveTextContent('Kho đang khoá')
  // A wrong one is refused in Vietnamese too, not in the core's English.
  await user.type(field('Mật khẩu kho'), 'mastre{Enter}')
  expect(await screen.findByRole('alert')).toHaveTextContent('Sai mật khẩu kho, hoặc tệp kho bị hỏng')
  expect(screen.getByRole('alert')).not.toHaveTextContent('wrong passphrase')
  await user.type(field('Mật khẩu kho'), 'master')
  await user.click(button('Mở kho'))
  expect(await screen.findByRole('button', { name: 'Đổi mật khẩu kho' })).toBeInTheDocument()
  expect(screen.getByRole('dialog')).toHaveTextContent('Kho đã mở')
  expect(screen.getByRole('dialog')).toHaveTextContent('Máy chủ 4')
  expect(screen.getByRole('dialog')).toHaveTextContent('Mật khẩu 2')
  expect(button('Xuất bản sao lưu')).toBeInTheDocument()
})

test('an empty passphrase that got past the form is refused in the window\'s language, and the shell is not asked', async () => {
  shell('none')
  // The fields are `required`: this is a webview that sends the form all the same.
  const send = () => screen.getByRole('dialog').querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }))
  send()
  expect(await screen.findByRole('alert')).toHaveTextContent('The passphrase must not be empty')
  setLang('vi')
  send()
  await vi.waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Mật khẩu kho không được để trống'))
  expect(asked('vault_unlock')).toEqual([])
  cleanup()

  // The same for a new one, of a vault that is open.
  setLang('en')
  shell('unlocked')
  await screen.findByText('Change passphrase')
  send()
  expect(await screen.findByRole('alert')).toHaveTextContent('The passphrase must not be empty')
  expect(asked('vault_pass_change')).toEqual([])
})
