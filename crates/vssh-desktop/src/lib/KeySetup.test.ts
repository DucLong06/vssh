import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { KeyStep } from './api'
import errors from './fixtures/errors.json'
import shown from './fixtures/shown.json'
import { setLang } from './i18n.svelte'
import KeySetup from './KeySetup.svelte'

type Args = Record<string, unknown>
const auth = errors.find((error) => error.class === 'auth')!
const other = (words: string) => ({ class: 'other', fail: null, chain: [words] })
let calls: [cmd: string, args: Args][] = []
const asked = (cmd: string) => calls.filter((call) => call[0] === cmd).map((call) => call[1])

/** The wizard for web-01 over a shell whose setup ends when the test says so. */
function open() {
  const ends: { resolve: (key: string) => void; reject: (error: object) => void }[] = []
  const commands: Record<string, (args: Args) => unknown> = {
    ssh_key_setup: () => new Promise((resolve, reject) => ends.push({ resolve, reject })),
    resolve_conn_error: ({ error, lang }) => {
      const failed = error as typeof auth
      const at = errors.findIndex((known) => known.class === failed.class)
      const words = failed.chain[0]
      // The core's own words for a failure of the network; the shell's for anything else.
      return failed.class === 'other' ? { class: 'other', label: 'ERROR', message: words, chain: words } : shown[lang as 'en' | 'vi'][at]
    },
  }
  mockIPC(
    (cmd, args) => {
      calls.push([cmd, args as Args])
      return commands[cmd](args as Args)
    },
    { shouldMockEvents: true },
  )
  const onclose = vi.fn()
  render(KeySetup, { host: 'web-01', onclose })
  return { onclose, user: userEvent.setup(), end: () => ends.at(-1)! }
}
/** What the shell says about a step, in the order given. */
async function says(step: KeyStep['step'], ...states: KeyStep['state'][]) {
  for (const state of states) await emit('key-setup-step', { host: 'web-01', step, state })
}
/** Each step's state and its name. */
const steps = () => screen.getAllByRole('listitem').map((li) => `${li.dataset.state} ${li.querySelector('strong')?.textContent}`)
const button = (name: string) => screen.getByRole('button', { name })

afterEach(async () => {
  cleanup()
  await new Promise((done) => setTimeout(done))
  clearMocks()
  calls = []
})

test('four steps, each with what it does; nothing is done before Start', async () => {
  open()
  const dialog = screen.getByRole('dialog', { name: 'Set up SSH key web-01' })
  expect(steps()).toEqual([
    'pending Create an Ed25519 key',
    'pending Add it to authorized_keys',
    'pending Sign in with the new key',
    'pending Turn off password login',
  ])
  expect(dialog).toHaveTextContent('A new key for this host alone, kept next to your ssh config.')
  expect(dialog).toHaveTextContent('sshd_config is backed up first, then the public key goes on the host.')
  expect(dialog).toHaveTextContent('A second connection, with the key and nothing else.')
  expect(dialog).toHaveTextContent('Only now that the key is known to work.')
  expect(dialog).toHaveTextContent('It needs root on the host, or sudo that asks no password.')
  expect(asked('ssh_key_setup')).toEqual([])
})

test('the steps are followed as the shell reports them: a spinner while one runs, a check when it is done', async () => {
  const { user, end, onclose } = open()
  await user.click(button('Start'))
  expect(asked('ssh_key_setup')).toEqual([{ host: 'web-01' }])
  // Once is enough.
  expect(screen.queryByRole('button', { name: 'Start' })).not.toBeInTheDocument()

  await says('generate', 'running')
  await vi.waitFor(() => expect(steps()[0]).toBe('running Create an Ed25519 key'))
  expect(screen.getAllByRole('progressbar', { name: 'running' })).toHaveLength(1)
  await says('generate', 'done')
  await vi.waitFor(() => expect(steps()[0]).toBe('done Create an Ed25519 key'))
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()

  // The backup of sshd_config is the beginning of the second step, not its end.
  await says('backup', 'running')
  await vi.waitFor(() => expect(steps()[1]).toBe('running Add it to authorized_keys'))
  await says('backup', 'done')
  await says('install', 'running')
  expect(steps()[1]).toBe('running Add it to authorized_keys')
  await says('install', 'done')
  await vi.waitFor(() => expect(steps()[1]).toBe('done Add it to authorized_keys'))
  // Another host's setup is not this one's.
  await emit('key-setup-step', { host: 'web-02', step: 'verify', state: 'failed' })
  await says('verify', 'running', 'done')
  await says('disable', 'running')
  await vi.waitFor(() =>
    expect(steps()).toEqual([
      'done Create an Ed25519 key',
      'done Add it to authorized_keys',
      'done Sign in with the new key',
      'running Turn off password login',
    ]),
  )
  expect(screen.queryByRole('status')).not.toBeInTheDocument()

  await says('disable', 'done')
  end().resolve('/home/me/.ssh/vssh_web-01_ed25519')
  expect(await screen.findByRole('status')).toHaveTextContent('web-01 is signed in to with its key from now on.')
  expect(screen.getByRole('status')).toHaveTextContent('/home/me/.ssh/vssh_web-01_ed25519')
  expect(steps().every((step) => step.startsWith('done '))).toBe(true)
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  await user.click(button('close'))
  expect(onclose).toHaveBeenCalledOnce()
})

test('a key that is refused at the third step: sshd_config is put back, and the dialog says access is as it was', async () => {
  const { user, end } = open()
  await user.click(button('Start'))
  await says('generate', 'running', 'done')
  await says('backup', 'running', 'done')
  await says('install', 'running', 'done')
  await says('verify', 'running', 'failed')
  await says('restore', 'running')
  await vi.waitFor(() => expect(steps()[2]).toBe('failed Sign in with the new key'))
  // Not yet: it is being put back.
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
  await says('restore', 'done')
  end().reject(auth)

  expect(await screen.findByRole('alert')).toHaveTextContent('✗ AUTH Public key rejected for deploy@10.0.0.5: 2 offered, none accepted')
  expect(await screen.findByRole('status')).toHaveTextContent('sshd_config was restored as it was — your access is unchanged')
  // The fourth step was never begun.
  expect(steps()).toEqual([
    'done Create an Ed25519 key',
    'done Add it to authorized_keys',
    'failed Sign in with the new key',
    'pending Turn off password login',
  ])
  expect(screen.queryByText(/signed in to with its key/)).not.toBeInTheDocument()

  // Again, from the first step.
  await user.click(button('try again'))
  expect(asked('ssh_key_setup')).toHaveLength(2)
  expect(steps().every((step) => step.startsWith('pending '))).toBe(true)
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
})

test('a failure before the host was touched has nothing to restore, and one whose restore failed does not say it was', async () => {
  const { user, end } = open()
  await user.click(button('Start'))
  await says('generate', 'running', 'done')
  await says('backup', 'running', 'failed')
  end().reject(other('cannot back up sshd_config (it takes root, or sudo without a password)'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR It needs root on the host, or sudo that asks no password.')
  expect(screen.getByRole('alert')).toHaveAttribute('title', 'cannot back up sshd_config (it takes root, or sudo without a password)')
  expect(steps().slice(0, 2)).toEqual(['done Create an Ed25519 key', 'failed Add it to authorized_keys'])
  expect(screen.queryByRole('status')).not.toBeInTheDocument()

  await user.click(button('try again'))
  await says('generate', 'running', 'done')
  await says('backup', 'running', 'done')
  await says('install', 'running', 'done')
  await says('verify', 'running', 'done')
  await says('disable', 'running', 'failed')
  await says('restore', 'running', 'failed')
  end().reject(other('sshd_config could NOT be restored from /etc/ssh/sshd_config.vssh.bak: check it by hand'))
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ ERROR It needs root on the host, or sudo that asks no password.')
  expect(screen.getByRole('alert')).toHaveAttribute('title', expect.stringContaining('sshd_config could NOT be restored'))
  expect(steps()[3]).toBe('failed Turn off password login')
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
})

test('in Vietnamese: the steps in plain words, and the word that access has not changed', async () => {
  setLang('vi')
  const { user, end } = open()
  const dialog = screen.getByRole('dialog', { name: 'Cài khoá SSH web-01' })
  expect(steps()).toEqual([
    'pending Tạo khoá Ed25519',
    'pending Thêm vào authorized_keys',
    'pending Thử đăng nhập bằng khoá mới',
    'pending Tắt đăng nhập bằng mật khẩu',
  ])
  expect(dialog).toHaveTextContent('Sao lưu sshd_config trước, rồi đưa khoá công khai lên máy chủ.')
  await user.click(button('Bắt đầu'))
  await says('generate', 'running', 'done')
  await says('backup', 'running', 'done')
  await says('install', 'running', 'done')
  await says('verify', 'running', 'failed')
  await says('restore', 'running', 'done')
  end().reject(auth)
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ XÁC THỰC Khoá công khai bị từ chối cho deploy@10.0.0.5')
  expect(await screen.findByRole('status')).toHaveTextContent('Đã khôi phục sshd_config như cũ — truy cập của bạn không đổi')
  expect(button('thử lại')).toBeInTheDocument()
})

test('in Vietnamese a refused backup is said in Vietnamese; a failure the core has words for keeps them', async () => {
  setLang('vi')
  const { user, end } = open()
  await user.click(button('Bắt đầu'))
  await says('generate', 'running', 'done')
  await says('backup', 'running', 'failed')
  end().reject(other('cannot back up sshd_config (it takes root, or sudo without a password)'))
  expect(await screen.findByRole('alert')).toHaveTextContent('Cần quyền root trên máy chủ, hoặc sudo không hỏi mật khẩu.')
  expect(screen.getByRole('alert')).not.toHaveTextContent('cannot back up')

  // The host could not be reached for the backup: that is not a matter of root.
  await user.click(button('thử lại'))
  await says('generate', 'running', 'done')
  await says('backup', 'running', 'failed')
  end().reject(auth)
  expect(await screen.findByRole('alert')).toHaveTextContent('✗ XÁC THỰC Khoá công khai bị từ chối cho deploy@10.0.0.5')
})
