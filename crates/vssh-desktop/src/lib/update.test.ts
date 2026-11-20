import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { setLang } from './i18n.svelte'
import Statusbar from './Statusbar.svelte'
import { check, forget, install, newer, update } from './update.svelte'
import Updated from './Updated.svelte'

/** Every command the window sent. */
let calls: string[] = []
/** GitHub, answering what a test says; never the network. */
const fetched = vi.fn<typeof fetch>()
const latest = (tag: unknown, ok = true, assets?: unknown) =>
  fetched.mockResolvedValue({ ok, json: async () => ({ tag_name: tag, assets }) } as Response)
/** What a release has, as GitHub lists it: the terminal app's packages and the desktop's. */
const assets = (ver: string) =>
  [`vssh_${ver}_amd64.deb`, `vssh-desktop_${ver}_amd64.AppImage`, `vssh-desktop_${ver}_amd64.deb`, `vssh-desktop-${ver}-1.x86_64.rpm`].map((name, at) => ({
    name,
    size: 5038776 + at - 2,
    browser_download_url: `https://github.com/DucLong06/vssh/releases/download/v${ver}/${name}`,
  }))
const DEB = '/home/long/.local/share/vssh/update/vssh-desktop_0.3.0_amd64.deb'
/** What the shell throws when a tool failed. */
const failing = (...chain: string[]) => () => {
  throw { class: 'other', fail: null, chain }
}

/** The shell: this build is 0.2.0 and came as a .deb; `over` replaces single commands. Each answer is what was asked for: the tag. */
function shell(over: Record<string, (args: Record<string, unknown>) => unknown> = {}) {
  const commands: Record<string, (args: Record<string, unknown>) => unknown> = {
    'plugin:app|version': () => '0.2.0',
    update_possible: () => true,
    update_download: () => null,
    update_install: () => null,
    ...over,
  }
  mockIPC(
    (cmd, args) => {
      calls.push(cmd)
      return commands[cmd]?.(args as Record<string, unknown>) ?? null
    },
    { shouldMockEvents: true },
  )
}

beforeEach(() => {
  vi.stubGlobal('fetch', fetched)
  shell()
})
afterEach(() => {
  cleanup()
  clearMocks()
  vi.unstubAllGlobals()
  fetched.mockReset()
  forget()
  calls = []
})

const bar = () => render(Statusbar, { hosts: 4, snippets: 1, sessions: 0, onsnippets: vi.fn() })

test('a version is newer by its numbers, with a v in front or without', () => {
  expect(newer('v0.2.1', '0.2.0')).toBe(true)
  expect(newer('v0.10.0', '0.9.9')).toBe(true)
  expect(newer('1.0', '0.9.9')).toBe(true)
  expect(newer('v0.2.0.1', '0.2.0')).toBe(true)
  expect(newer('v0.2.0', '0.2.0')).toBe(false)
  expect(newer('v0.2', '0.2.0')).toBe(false)
  expect(newer('v0.1.9', '0.2.0')).toBe(false)
})

test('a newer release: one line in the status bar, which opens the releases and downloads nothing', async () => {
  latest('v0.3.0')
  bar()
  expect(screen.queryByRole('status')).toBeNull()
  await check()
  expect(update.tag).toBe('v0.3.0')
  const line = await screen.findByRole('button', { name: 'New version v0.3.0 — see it on GitHub' })
  await userEvent.click(line)
  expect(calls).toEqual(['plugin:app|version', 'open_releases'])
  // A release with no .deb of the desktop app has no button, and the shell is not asked what the app came as.
  expect(screen.queryByRole('button', { name: 'Update' })).toBeNull()
  expect(fetched).toHaveBeenCalledOnce()
})

test('the question is one public GET: no header, no body, no cookie, nothing of the user in the address', async () => {
  latest('v0.3.0')
  await check()
  const [url, init] = fetched.mock.calls[0]
  expect(url).toBe('https://api.github.com/repos/DucLong06/vssh/releases/latest')
  const { signal, ...rest } = init!
  expect(signal).toBeInstanceOf(AbortSignal)
  expect(rest).toEqual({ credentials: 'omit', referrerPolicy: 'no-referrer', cache: 'no-store' })
})

test('no newer release: the same version, or an older one, says nothing', async () => {
  bar()
  for (const tag of ['v0.2.0', '0.2.0', 'v0.1.9']) {
    latest(tag)
    await check()
    expect(update.tag).toBeNull()
  }
  expect(fetched).toHaveBeenCalledTimes(3)
  expect(screen.queryByRole('status')).toBeNull()
  expect(screen.queryByRole('button', { name: /New version/ })).toBeNull()
})

test('no network, a dead proxy, a refusal, an answer that is no release: silence, and no rejection', async () => {
  bar()
  fetched.mockRejectedValue(new TypeError('Load failed'))
  await expect(check()).resolves.toBeUndefined()
  latest('v9.9.9', false)
  await expect(check()).resolves.toBeUndefined()
  fetched.mockResolvedValue({ ok: true, json: () => Promise.reject(new SyntaxError('not JSON')) } as unknown as Response)
  await expect(check()).resolves.toBeUndefined()
  // Only what looks like a version is put on screen.
  for (const tag of [undefined, 9, '', 'nightly', 'v1.0.0 <b>now</b>', 'v1.0.0-rc.1']) {
    latest(tag)
    await expect(check()).resolves.toBeUndefined()
  }
  expect(update.tag).toBeNull()
  expect(screen.queryByRole('status')).toBeNull()
  expect(screen.queryByRole('alert')).toBeNull()
  expect(calls).not.toContain('open_releases')
})

test('dismissed, the line is gone for as long as the app runs; a browser that does not open is no error', async () => {
  latest('v0.3.0')
  mockIPC((cmd) => {
    if (cmd === 'open_releases') throw { class: 'other', fail: null, chain: ['xdg-open could not open'] }
    return '0.2.0'
  })
  bar()
  await check()
  await userEvent.click(await screen.findByRole('button', { name: /New version v0.3.0/ }))
  expect(screen.queryByRole('alert')).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))
  expect(update.tag).toBeNull()
  expect(screen.queryByRole('button', { name: /New version/ })).toBeNull()
  expect(fetched).toHaveBeenCalledOnce()
})

test('in Vietnamese', async () => {
  setLang('vi')
  latest('v0.3.0')
  bar()
  await check()
  expect(await screen.findByRole('button', { name: 'Có bản mới v0.3.0 — xem trên GitHub' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Tắt thông báo' })).toBeInTheDocument()
})

/** The status bar, once the app has heard of v0.3.0 and its files. */
async function offered() {
  latest('v0.3.0', true, assets('0.3.0'))
  bar()
  await check()
  return within(await screen.findByRole('status'))
}

test('a newer release with the .deb: Update fetches it, installs it and offers to restart; nothing is fetched before the click', async () => {
  const line = await offered()
  expect(update.deb).toBe(5038776)
  expect(line.getByRole('button', { name: 'New version v0.3.0 — see it on GitHub' })).toBeInTheDocument()
  expect(calls).toEqual(['plugin:app|version', 'update_possible'])
  expect(fetched).toHaveBeenCalledOnce()

  await userEvent.click(line.getByRole('button', { name: 'Update' }))
  await waitFor(() => expect(update.phase).toBe('installed'))
  expect(calls).toEqual(['plugin:app|version', 'update_possible', 'update_download', 'update_install'])
  // GitHub was asked the one question, and the download was the shell's.
  expect(fetched).toHaveBeenCalledOnce()
  expect(update.dialog).toBe(true)

  // The dialog the app opens over it.
  render(Updated)
  const dialog = within(screen.getByRole('dialog', { name: 'Update' }))
  expect(dialog.getByText('Installed v0.3.0 — restart to use it')).toBeInTheDocument()
  // Later: the dialog goes, the line stays and still has the way to restart.
  await userEvent.click(dialog.getByRole('button', { name: 'Later' }))
  expect(update.dialog).toBe(false)
  expect(calls).not.toContain('update_restart')
  expect(line.getByText('Installed v0.3.0 — restart to use it')).toBeInTheDocument()
  expect(line.queryByRole('button', { name: 'Update' })).toBeNull()
  await userEvent.click(line.getByRole('button', { name: 'Restart' }))
  expect(calls.at(-1)).toBe('update_restart')
})

test('the dialog\'s Restart ends the app and starts it again', async () => {
  await offered()
  await install()
  render(Updated)
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Restart' }))
  expect(calls.at(-1)).toBe('update_restart')
})

test('the shell is told the tag and nothing else: no address, no path', async () => {
  const told: unknown[] = []
  shell({ update_download: (args) => void told.push(args), update_install: (args) => (told.push(args), null) })
  await offered()
  await install()
  expect(told).toEqual([{ tag: 'v0.3.0' }, { tag: 'v0.3.0' }])
})

test('while it downloads the line says how much is here, of how much', async () => {
  let arrived = () => {}
  shell({ update_download: () => new Promise<null>((done) => (arrived = () => done(null))) })
  const line = await offered()
  await userEvent.click(line.getByRole('button', { name: 'Update' }))
  expect(await line.findByText('Downloading v0.3.0 — 0 B / 4.8 M')).toBeInTheDocument()
  // No second download from a second click: there is nothing to click.
  expect(line.queryAllByRole('button')).toEqual([])
  await install()
  await emit('update-progress', 1572864)
  expect(await line.findByText('Downloading v0.3.0 — 1.5 M / 4.8 M')).toBeInTheDocument()
  expect(calls.filter((cmd) => cmd === 'update_download')).toHaveLength(1)

  let installed = () => {}
  shell({ update_install: () => new Promise<null>((done) => (installed = () => done(null))) })
  arrived()
  expect(await line.findByText('Installing v0.3.0 — the system asks for your password')).toBeInTheDocument()
  installed()
  expect(await line.findByText('Installed v0.3.0 — restart to use it')).toBeInTheDocument()
})

test('the button is there only for a newer release that has the .deb, in an app that came as one', async () => {
  bar()
  const button = () => screen.queryByRole('button', { name: 'Update' })
  // Not newer.
  latest('v0.2.0', true, assets('0.2.0'))
  await check()
  expect(update.tag).toBeNull()
  expect(button()).toBeNull()
  expect(calls).toEqual(['plugin:app|version'])
  // Newer, with the .deb: there it is.
  latest('v0.3.0', true, assets('0.3.0'))
  await check()
  await waitFor(() => expect(button()).toBeInTheDocument())
  // Newer, but the release has no .deb of the desktop app, or says nothing of its size, or its files are no list.
  for (const files of [assets('0.3.0').filter(({ name }) => !name.endsWith('amd64.deb') || name.startsWith('vssh_')), [{ name: 'vssh-desktop_0.3.0_amd64.deb' }], 'none', [null]]) {
    forget()
    latest('v0.3.0', true, files)
    await check()
    expect(await screen.findByRole('button', { name: /New version v0.3.0/ })).toBeInTheDocument()
    expect(button()).toBeNull()
  }
  // Newer, with the .deb, but this app is an AppImage, an .rpm or a build of one's own; or the shell cannot say.
  for (const possible of [() => false, failing('no dpkg-query')]) {
    forget()
    shell({ update_possible: possible })
    latest('v0.3.0', true, assets('0.3.0'))
    await check()
    expect(await screen.findByRole('button', { name: /New version v0.3.0/ })).toBeInTheDocument()
    expect(button()).toBeNull()
    expect(update.deb).toBeNull()
  }
  // And with no button there is nothing to start it: nothing was fetched or installed in all of this.
  expect(calls).not.toContain('update_download')
  expect(calls).not.toContain('update_install')
})

test('a download that fails: a short error with Retry, nothing installed; the retry downloads again', async () => {
  shell({ update_download: failing('wget could not fetch it: Unable to establish SSL connection.') })
  const line = await offered()
  await userEvent.click(line.getByRole('button', { name: 'Update' }))
  const error = await line.findByRole('alert')
  expect(error).toHaveTextContent('Could not download v0.3.0')
  expect(error).toHaveAttribute('title', 'wget could not fetch it: Unable to establish SSL connection.')
  expect(calls).not.toContain('update_install')
  expect(update.dialog).toBe(false)

  shell()
  await userEvent.click(line.getByRole('button', { name: 'Retry' }))
  await waitFor(() => expect(update.phase).toBe('installed'))
  expect(calls.filter((cmd) => cmd.startsWith('update_'))).toEqual(['update_possible', 'update_download', 'update_download', 'update_install'])
  expect(line.queryByRole('alert')).toBeNull()
})

test('dpkg fails: the error says it was the install, with dpkg\'s words; it can be retried, or dismissed', async () => {
  shell({ update_install: failing('dpkg could not install the package', 'dpkg: error: dpkg frontend lock was locked by another process with pid 4242') })
  const line = await offered()
  await userEvent.click(line.getByRole('button', { name: 'Update' }))
  const error = await line.findByRole('alert')
  expect(error).toHaveTextContent('Could not install v0.3.0')
  expect(error).toHaveAttribute('title', 'dpkg could not install the package: dpkg: error: dpkg frontend lock was locked by another process with pid 4242')
  expect(update.dialog).toBe(false)
  expect(line.queryByRole('button', { name: 'Restart' })).toBeNull()

  await userEvent.click(line.getByRole('button', { name: 'Retry' }))
  expect(await line.findByRole('alert')).toHaveTextContent('Could not install v0.3.0')
  expect(calls.filter((cmd) => cmd === 'update_install')).toHaveLength(2)
  await userEvent.click(line.getByRole('button', { name: 'Dismiss' }))
  expect(screen.queryByRole('status')).toBeNull()
  expect(update).toMatchObject({ tag: null, phase: null })
})

test('nobody to ask for the password: nothing is installed, and the dialog has the line that installs it by hand', async () => {
  shell({ update_install: () => DEB })
  const line = await offered()
  await userEvent.click(line.getByRole('button', { name: 'Update' }))
  await waitFor(() => expect(update.dialog).toBe(true))
  expect(update.phase).toBe('manual')
  render(Updated)
  const dialog = within(screen.getByRole('dialog', { name: 'Update' }))
  expect(dialog.getByText(/^v0.3.0 is downloaded, but the password could not be asked for here, so nothing was installed\./)).toBeInTheDocument()
  expect(dialog.getByText(`sudo apt install '${DEB}'`)).toBeInTheDocument()
  // It is not installed: no word of a restart.
  expect(dialog.queryByRole('button', { name: 'Restart' })).toBeNull()
  await userEvent.click(dialog.getByRole('button', { name: 'close' }))
  expect(update.dialog).toBe(false)
  // The line is as it was, and can be asked again.
  expect(line.getByRole('button', { name: 'Update' })).toBeInTheDocument()
  expect(line.queryByRole('button', { name: 'Restart' })).toBeNull()
  expect(calls).not.toContain('update_restart')
})

test('a path with a quote in it is still one word to the shell', async () => {
  shell({ update_install: () => "/home/o'brien/vssh-desktop_0.3.0_amd64.deb" })
  await offered()
  await install()
  render(Updated)
  expect(screen.getByText(`sudo apt install '/home/o'\\''brien/vssh-desktop_0.3.0_amd64.deb'`)).toBeInTheDocument()
})

test('the update, in Vietnamese', async () => {
  setLang('vi')
  let arrived = () => {}
  shell({ update_download: () => new Promise<null>((done) => (arrived = () => done(null))), update_install: failing('dpkg') })
  const line = await offered()
  await userEvent.click(line.getByRole('button', { name: 'Cập nhật' }))
  expect(await line.findByText('Đang tải v0.3.0 — 0 B / 4.8 M')).toBeInTheDocument()
  arrived()
  expect(await line.findByRole('alert')).toHaveTextContent('Không cài được v0.3.0')
  shell({ update_download: failing('wget') })
  await userEvent.click(line.getByRole('button', { name: 'Thử lại' }))
  expect(await line.findByText('Không tải được v0.3.0')).toBeInTheDocument()
  shell({ update_install: () => DEB })
  await userEvent.click(line.getByRole('button', { name: 'Thử lại' }))
  await waitFor(() => expect(update.phase).toBe('manual'))
  const { unmount } = render(Updated)
  expect(within(screen.getByRole('dialog', { name: 'Cập nhật' })).getByText(/^Đã tải v0.3.0, nhưng ở đây không hỏi được mật khẩu nên chưa cài gì\./)).toBeInTheDocument()
  unmount()
  shell()
  await userEvent.click(line.getByRole('button', { name: 'Cập nhật' }))
  expect(await line.findByText('Đã cài v0.3.0 — khởi động lại để dùng')).toBeInTheDocument()
  expect(line.getByRole('button', { name: 'Khởi động lại' })).toBeInTheDocument()
  render(Updated)
  expect(within(screen.getByRole('dialog', { name: 'Cập nhật' })).getByRole('button', { name: 'Để sau' })).toBeInTheDocument()
})
