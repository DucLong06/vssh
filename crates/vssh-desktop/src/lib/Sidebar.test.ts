import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import type { Host } from './api'
import fixture from './fixtures/hosts.json'
import { i18n, setLang } from './i18n.svelte'
import { scale } from './scale.svelte'
import { streamer } from './streamer.svelte'
import { theme } from './theme.svelte'
import Sidebar from './Sidebar.svelte'

const hosts: Host[] = fixture
const props = (over = {}) => ({ hosts, ...over })

test('no hosts: none counted; the dashboard is where it says what to do', () => {
  render(Sidebar, props({ hosts: [] }))
  expect(screen.getByRole('button', { name: 'Hosts 0' })).toBeInTheDocument()
  expect(screen.getByText('no tags')).toBeInTheDocument()
})

// --- D12: the hosts are the view's in the middle -------------------------------

test('the rail has no list of hosts: their row counts them, and not one is named, in full or narrow', async () => {
  const { rerender } = render(Sidebar, props())
  const nav = screen.getByRole('navigation')
  expect(screen.getByRole('button', { name: 'Hosts 4' })).toBeInTheDocument()
  expect(screen.queryByRole('list', { name: 'Hosts' })).not.toBeInTheDocument()
  for (const host of hosts) expect(nav, host.alias).not.toHaveTextContent(host.alias)
  expect(nav.textContent).not.toMatch(/10\.0\.0\.|@/)
  await rerender(props({ compact: true }))
  expect(screen.queryByRole('list')).toBeNull()
  for (const host of hosts) expect(nav, host.alias).not.toHaveTextContent(host.alias)
})

test('tags and groups are counted, and one click narrows what the hosts\' row counts', async () => {
  const user = userEvent.setup()
  render(Sidebar, props())
  const tags = within(screen.getByRole('list', { name: 'Tags' })).getAllByRole('button')
  // Most used first, then by name; `web,us` and `web eu` are the same tag.
  expect(tags.map((tag) => tag.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
    '#eu 2',
    '#prod 2',
    '#web 2',
    '#db 1',
    '#us 1',
  ])

  const db = screen.getByRole('button', { name: '#db 1' })
  expect(db).toHaveAttribute('aria-pressed', 'false')
  await user.click(db)
  expect(db).toHaveAttribute('aria-pressed', 'true')
  expect(screen.getByRole('button', { name: 'Hosts 1' })).toBeInTheDocument()

  await user.click(db)
  expect(screen.getByRole('button', { name: 'Hosts 4' })).toBeInTheDocument()
})

test('open sessions are listed with their state; a click goes to the tab', async () => {
  const user = userEvent.setup()
  const onsession = vi.fn()
  const sessions = [
    { id: 1, title: 'web-01', state: 'up', busy: null },
    { id: 4, title: 'deploy@10.0.0.9', state: 'closed', busy: null },
  ]
  const { rerender } = render(Sidebar, props({ sessions, active: 4, onsession }))
  expect(screen.getByRole('heading', { name: 'Sessions 2' })).toBeInTheDocument()
  const [first, second] = within(screen.getByRole('list', { name: 'Sessions' })).getAllByRole('button')
  expect(first).toHaveAccessibleName('connected web-01')
  expect(second).toHaveAccessibleName('disconnected deploy@10.0.0.9')
  expect(second).toHaveAttribute('aria-current', 'true')
  expect(first).not.toHaveAttribute('aria-current')
  await user.click(first)
  expect(onsession).toHaveBeenCalledWith(1)
  // With none open there is nothing to list, and no heading for it.
  await rerender(props({ sessions: [] }))
  expect(screen.queryByRole('heading', { name: /Sessions/ })).not.toBeInTheDocument()
})

test('in Vietnamese the words are the TUI\'s', () => {
  setLang('vi')
  render(Sidebar, props({ hosts: [] }))
  expect(screen.getByRole('button', { name: 'Máy chủ 0' })).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: 'Thẻ 0' })).toBeInTheDocument()
  expect(screen.getByText('chưa có thẻ')).toBeInTheDocument()
})

test('the tag chosen is the caller\'s to use: the dashboard filters by it too', async () => {
  const user = userEvent.setup()
  let tag: string | null = null
  render(Sidebar, {
    ...props(),
    get tag() {
      return tag
    },
    set tag(value) {
      tag = value
    },
  })
  await user.click(screen.getByRole('button', { name: '#db 1' }))
  expect(tag).toBe('db')
  await user.click(screen.getByRole('button', { name: '#db 1' }))
  expect(tag).toBeNull()
})

// --- D9: the rail ------------------------------------------------------------

const rail = (over = {}) => {
  const heard = { onnav: vi.fn(), onvault: vi.fn() }
  const sessions = [{ id: 1, title: 'web-01', state: 'up', busy: null }]
  render(Sidebar, props({ sessions, snippets: 3, ...heard, ...over }))
  return heard
}
const item = (name: string) => screen.getByRole('button', { name })

test('the rail leads everywhere: each row has an icon and a name, and a click asks for its view', async () => {
  const user = userEvent.setup()
  const { onnav, onvault } = rail()
  const rows = { Status: 'dashboard', 'Hosts 4': 'hosts', 'Sessions 1': 'sessions', Files: 'files', 'Snippets 3': 'snippets', Forwards: 'forwards' }
  for (const [name, to] of Object.entries(rows)) {
    expect(item(name).querySelector('svg'), name).toHaveAttribute('aria-hidden', 'true')
    await user.click(item(name))
    expect(onnav, name).toHaveBeenLastCalledWith(to)
  }
  expect(onnav).toHaveBeenCalledTimes(6)
  // The vault is the last of them, and has its own way in.
  expect(item('Vault').querySelector('svg')).not.toBeNull()
  await user.click(item('Vault'))
  expect(onvault).toHaveBeenCalledOnce()
  // In the order of the concept.
  const order = within(screen.getByRole('navigation')).getAllByRole('button').map((row) => row.textContent?.replace(/\s+/g, ' ').trim())
  const names = ['Status', 'Hosts 4', 'Sessions 1', 'Files', 'Snippets 3', 'Forwards', 'Vault']
  expect(order.filter((name) => names.includes(name!))).toEqual(names)
})

test('the view in front is the current page of the rail', () => {
  rail({ current: 'sessions' })
  expect(item('Sessions 1')).toHaveAttribute('aria-current', 'page')
  expect(item('Status')).not.toHaveAttribute('aria-current')
})

test('files and forwards are a host\'s: with no host there is nowhere to go, nor to a session with none open', async () => {
  const user = userEvent.setup()
  const { onnav } = rail({ hosts: [], sessions: [] })
  for (const name of ['Files', 'Forwards', 'Sessions']) {
    expect(item(name), name).toBeDisabled()
    await user.click(item(name))
  }
  expect(onnav).not.toHaveBeenCalled()
  expect(item('Status')).toBeEnabled()
  expect(item('Snippets 3')).toBeEnabled()
})

test('with hosts, files and forwards are there to ask for whether or not one is selected: the window asks whose', async () => {
  const user = userEvent.setup()
  const { onnav } = rail({ sessions: [] })
  await user.click(item('Files'))
  expect(onnav).toHaveBeenLastCalledWith('files')
  await user.click(item('Forwards'))
  expect(onnav).toHaveBeenLastCalledWith('forwards')
})

test('the vault says what it is on its row', () => {
  rail({ vault: 'locked' })
  expect(item('Vault locked')).toBeInTheDocument()
})

test("the rail in Vietnamese: the table's words, and no other", () => {
  setLang('vi')
  rail()
  for (const name of ['Trạng thái', 'Máy chủ 4', 'Phiên 1', 'Tệp', 'Đoạn lệnh 3', 'Chuyển tiếp', 'Kho', 'Cài đặt']) {
    expect(item(name), name).toBeInTheDocument()
  }
  // The name of the app is drawn, not said: it is the same in every language.
  expect(screen.getByRole('navigation').textContent).not.toMatch(/vssh/)
})

test('forwards that listen are counted on their row, also with the dialog shut; none: no number', () => {
  const { rerender } = render(Sidebar, props({ tunnels: 2 }))
  expect(screen.getByRole('button', { name: 'Forwards 2' })).toBeInTheDocument()
  void rerender(props({ tunnels: 0 }))
  expect(screen.getByRole('button', { name: 'Forwards' })).toBeInTheDocument()
})

test('a tag is grey until the list is narrowed to it: colour is for what is chosen, and for a state', async () => {
  const user = userEvent.setup()
  render(Sidebar, props())
  const prod = screen.getByRole('button', { name: /#prod/ })
  expect(prod.style.color).toBe('')
  await user.click(prod)
  expect(prod.style.color).toMatch(/^var\(--color-/)
  expect(screen.getByRole('button', { name: /#web/ }).style.color).toBe('')
})

// --- D11: the rail narrow, its search, and the settings in one place ------------

test('compact: the rail is its icons, each named in a tooltip and to a reader of the screen; no lists, and the numbers at the icons\' corners', async () => {
  const user = userEvent.setup()
  const oncompact = vi.fn()
  const { onnav } = rail({ compact: true, oncompact, tunnels: 2, onpalette: vi.fn() })
  const nav = screen.getByRole('navigation')
  expect(nav).toHaveAttribute('data-compact')
  // Every row is still there, by the same name, and goes where it went.
  for (const name of ['Status', 'Hosts 4', 'Sessions 1', 'Files', 'Snippets 3', 'Forwards 2', 'Vault', 'Settings']) {
    expect(item(name).querySelector('svg'), name).not.toBeNull()
  }
  expect(item('Sessions 1')).toHaveAttribute('title', 'Sessions')
  expect(item('Sessions 1').querySelector('span')).toHaveClass('sr-only')
  expect(item('Command palette')).toHaveAttribute('title', 'Command palette (Ctrl+K, Ctrl+Shift+K)')
  expect(item('Command palette')).not.toHaveTextContent(/Search/)
  await user.click(item('Forwards 2'))
  expect(onnav).toHaveBeenLastCalledWith('forwards')
  // What an icon cannot hold is not there: the hosts, the sessions, the tags.
  expect(screen.queryByRole('list')).toBeNull()
  expect(nav).not.toHaveTextContent(/web-01|#prod|Tags/i)
  // Its corner's button brings all of it back.
  const wide = item('Expand the sidebar')
  expect(wide).toHaveAttribute('aria-pressed', 'true')
  await user.click(wide)
  expect(oncompact).toHaveBeenLastCalledWith(false)
})

test('in full the corner\'s button asks for the narrow rail, and the rows say their names', async () => {
  const user = userEvent.setup()
  const oncompact = vi.fn()
  rail({ oncompact })
  expect(screen.getByRole('navigation')).not.toHaveAttribute('data-compact')
  const narrow = item('Compact sidebar: icons only')
  expect(narrow).toHaveAttribute('aria-pressed', 'false')
  await user.click(narrow)
  expect(oncompact).toHaveBeenLastCalledWith(true)
  expect(item('Sessions 1')).not.toHaveAttribute('title')
  expect(screen.getByRole('list', { name: 'Sessions' })).toBeInTheDocument()
})

test('the rail\'s first row is the way to the palette, with its key', async () => {
  const user = userEvent.setup()
  const onpalette = vi.fn()
  rail({ onpalette })
  const search = item('Command palette')
  expect(search).toHaveTextContent('Search hosts and actions Ctrl+K')
  expect(search).toHaveAttribute('aria-keyshortcuts', 'Control+K')
  await user.click(search)
  expect(onpalette).toHaveBeenCalledOnce()
})

test('the settings at the rail\'s foot: the language, the theme and streamer mode say what they are and change at a click; focus mode and the cheatsheet with their keys', async () => {
  const user = userEvent.setup()
  const on = { onzen: vi.fn(), onshortcuts: vi.fn() }
  rail(on)
  const cog = item('Settings')
  expect(cog).toHaveAttribute('aria-expanded', 'false')
  const rows = () => within(screen.getByRole('menu')).getAllByRole('menuitem').map((row) => row.textContent?.replace(/\s+/g, ' ').trim())
  const choose = async (name: RegExp) => {
    await user.click(item('Settings'))
    await user.click(within(screen.getByRole('menu')).getByRole('menuitem', { name }))
    expect(screen.queryByRole('menu')).toBeNull()
  }
  await user.click(cog)
  expect(cog).toHaveAttribute('aria-expanded', 'true')
  expect(rows()).toEqual(['Language EN', 'Theme Night', 'A-', '100%', 'A+', 'Streamer mode (made-up addresses)', 'Focus mode Ctrl+Shift+B', 'Keyboard shortcuts ?'])
  // A second click on the cog closes it.
  await user.click(cog)
  expect(screen.queryByRole('menu')).toBeNull()
  await choose(/Language/)
  expect(i18n.lang).toBe('vi')
  await user.click(item('Cài đặt'))
  expect(rows()).toEqual(['Ngôn ngữ VI', 'Giao diện Tối', 'A-', '100%', 'A+', 'Chế độ phát trực tiếp (địa chỉ giả)', 'Chế độ tập trung Ctrl+Shift+B', 'Phím tắt ?'])
  // The words change with the next drawing: the key after it finds the rail in English.
  setLang('en')
  await user.keyboard('{Escape}')
  await choose(/Theme/)
  expect(theme.name).toBe('day')
  expect(localStorage.getItem('vssh.theme')).toBe('day')
  await choose(/Streamer mode/)
  expect(streamer.on).toBe(true)
  await user.click(item('Settings'))
  expect(rows()).toContain('Streamer mode (made-up addresses) ✓')
  expect(rows()).toContain('Theme Day')
  await user.keyboard('{Escape}')
  await choose(/Focus mode/)
  expect(on.onzen).toHaveBeenCalledOnce()
  await choose(/Keyboard shortcuts/)
  expect(on.onshortcuts).toHaveBeenCalledOnce()
})

test('the settings have the window\'s size: A- 100% A+, each a step, the menu open the while; a tooltip says the key, and an end is an end', async () => {
  const user = userEvent.setup()
  rail()
  await user.click(item('Settings'))
  const menu = within(screen.getByRole('menu'))
  const part = (name: string) => menu.getByRole('menuitem', { name })
  expect(menu.getByText('UI size')).toBeInTheDocument()
  expect(part('Zoom in UI')).toHaveAttribute('title', 'Zoom in UI (Ctrl+=, Ctrl++)')
  expect(part('Zoom out UI')).toHaveAttribute('title', 'Zoom out UI (Ctrl+-)')
  expect(part('Reset UI zoom')).toHaveAttribute('title', 'Reset UI zoom (Ctrl+0)')
  await user.click(part('Zoom in UI'))
  await user.click(part('Zoom in UI'))
  expect(part('Reset UI zoom')).toHaveTextContent('125%')
  expect(localStorage.getItem('vssh.ui_scale')).toBe('125')
  await user.click(part('Zoom in UI'))
  expect(part('Zoom in UI')).toHaveAttribute('aria-disabled', 'true')
  await user.click(part('Zoom in UI'))
  expect(scale.percent).toBe(150)
  await user.click(part('Reset UI zoom'))
  expect(part('Reset UI zoom')).toHaveTextContent('100%')
  for (let i = 0; i < 3; i++) await user.click(part('Zoom out UI'))
  expect(scale.percent).toBe(80)
  expect(part('Zoom out UI')).toHaveAttribute('aria-disabled', 'true')
  // Open all the while.
  expect(screen.getByRole('menu')).toBeInTheDocument()
})

test('the tags of the rail are small, and three whole rows of them at most', () => {
  render(Sidebar, props())
  expect(screen.getByRole('list', { name: 'Tags' })).toHaveClass('max-h-19')
  expect(screen.getByRole('button', { name: '#db 1' })).toHaveClass('text-3xs')
})
