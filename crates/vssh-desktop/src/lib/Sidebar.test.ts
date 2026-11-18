import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import type { Host } from './api'
import fixture from './fixtures/hosts.json'
import { setLang } from './i18n.svelte'
import Sidebar from './Sidebar.svelte'

const hosts: Host[] = fixture
const props = (over = {}) => ({ hosts, selected: null, onselect: vi.fn(), ...over })
const rows = () => within(screen.getByRole('list', { name: 'Hosts' })).getAllByRole('button')

test('no hosts: nothing listed; the dashboard is where it says what to do', () => {
  render(Sidebar, props({ hosts: [] }))
  expect(screen.getByRole('heading', { name: 'Hosts 0' })).toBeInTheDocument()
  expect(screen.queryByRole('list', { name: 'Hosts' })).not.toBeInTheDocument()
  expect(screen.getByText('no tags')).toBeInTheDocument()
})

test('many hosts: every one is listed with its address, in the order given', () => {
  render(Sidebar, props())
  expect(rows().map((row) => row.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
    '★ web-01 deploy@10.0.0.5:2222',
    'web-02 deploy@10.0.0.6',
    'db-primary postgres@db.internal.example.com',
    'bare',
  ])
  expect(screen.getByRole('img', { name: 'pinned' })).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: 'Hosts 4' })).toBeInTheDocument()
})

test('a host with nothing but a name shows the name and nothing made up', () => {
  const { container } = render(Sidebar, props({ hosts: hosts.filter((h) => h.alias === 'bare') }))
  expect(rows()).toHaveLength(1)
  expect(rows()[0]).toHaveTextContent(/^bare$/)
  expect(container.textContent).not.toMatch(/null|undefined|@|:/)
})

test('tags and groups are counted, and one click filters by them', async () => {
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
  expect(rows().map((row) => row.textContent?.trim().split(/\s+/)[0])).toEqual(['db-primary'])
  expect(screen.getByRole('heading', { name: 'Hosts 1' })).toBeInTheDocument()

  await user.click(db)
  expect(rows()).toHaveLength(4)
})

test('a click selects a host, and the selected one is marked', async () => {
  const user = userEvent.setup()
  const onselect = vi.fn()
  render(Sidebar, props({ onselect, selected: 'web-02' }))
  expect(screen.getByRole('button', { name: /web-02/ })).toHaveAttribute('aria-current', 'true')
  expect(screen.getByRole('button', { name: /db-primary/ })).not.toHaveAttribute('aria-current')
  await user.click(screen.getByRole('button', { name: /db-primary/ }))
  expect(onselect).toHaveBeenCalledExactlyOnceWith('db-primary')
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
  expect(screen.getByRole('heading', { name: 'Máy chủ 0' })).toBeInTheDocument()
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
  render(Sidebar, props({ selected: 'web-01', sessions, snippets: 3, ...heard, ...over }))
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

test("files and forwards are the selected host's: with none selected there is nowhere to go, nor to a session with none open", async () => {
  const user = userEvent.setup()
  const { onnav } = rail({ selected: null, sessions: [] })
  for (const name of ['Files', 'Forwards', 'Sessions']) {
    expect(item(name), name).toBeDisabled()
    await user.click(item(name))
  }
  expect(onnav).not.toHaveBeenCalled()
  expect(item('Status')).toBeEnabled()
  expect(item('Snippets 3')).toBeEnabled()
})

test('the vault says what it is on its row', () => {
  rail({ vault: 'locked' })
  expect(item('Vault locked')).toBeInTheDocument()
})

test("the rail in Vietnamese: the table's words, and no other", () => {
  setLang('vi')
  rail()
  for (const name of ['Trạng thái', 'Máy chủ 4', 'Phiên 1', 'Tệp', 'Đoạn lệnh 3', 'Chuyển tiếp', 'Kho', 'Giao diện: Tối']) {
    expect(item(name), name).toBeInTheDocument()
  }
  // The name of the app is drawn, not said: it is the same in every language.
  expect(screen.getByRole('navigation').textContent).not.toMatch(/vssh/)
})

test('forwards that listen are counted on their row, also with the dialog shut; none: no number', () => {
  const { rerender } = render(Sidebar, props({ selected: 'web-01', tunnels: 2 }))
  expect(screen.getByRole('button', { name: 'Forwards 2' })).toBeInTheDocument()
  void rerender(props({ selected: 'web-01', tunnels: 0 }))
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
