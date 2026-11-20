import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { fireEvent, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { ConnError, Host, Peek } from './api'
import Dashboard from './Dashboard.svelte'
import errors from './fixtures/errors.json'
import fixture from './fixtures/hosts.json'
import peekFixture from './fixtures/peek.json'
import { setLang } from './i18n.svelte'
import { forget, live, trail } from './live.svelte'

const hosts = fixture as Host[]
const peek = peekFixture as unknown as Peek
const props = (over = {}) => ({ hosts, onconnect: vi.fn(), onpalette: vi.fn(), ...over })
const cards = () => screen.queryAllByRole('article')
const names = () => cards().map((card) => card.getAttribute('aria-label'))
/** `n` hosts with nothing but a name. */
const many = (n: number): Host[] =>
  Array.from({ length: n }, (_, i) => ({ ...hosts[3], alias: `node-${String(i + 1).padStart(2, '0')}` }))

const SEARCH = 'Find a host, or type user@hostname to connect'
const headings = () => screen.queryAllByRole('heading', { level: 2 }).map((h) => h.textContent?.replace(/\s+/g, ' ').trim())
const cpu = (pct: number): Peek => ({ ...peek, stats: { cpu: pct, mem: null, disk: null } })
const sort = (name: string) => within(screen.getByRole('radiogroup', { name: 'Sort' })).getByRole('radio', { name })

afterEach(() => {
  forget()
  localStorage.clear()
})

test('no hosts: an empty state that says what to do, and opens the palette', async () => {
  const user = userEvent.setup()
  const onpalette = vi.fn()
  render(Dashboard, props({ hosts: [], onpalette }))
  expect(screen.getByRole('heading', { name: 'No hosts yet.' })).toBeInTheDocument()
  expect(screen.getByText(/connect once without saving/)).toBeInTheDocument()
  expect(cards()).toHaveLength(0)
  await user.click(screen.getByRole('button', { name: 'Command palette' }))
  expect(onpalette).toHaveBeenCalledOnce()
})

test('hosts that could not be read: the reason, not "no hosts"', () => {
  const error = { class: 'other' as const, label: 'ERROR', message: 'cannot read config', chain: '' }
  render(Dashboard, props({ hosts: [], error }))
  expect(screen.getByRole('alert')).toHaveTextContent('ERROR cannot read config')
  expect(screen.queryByText('No hosts yet.')).not.toBeInTheDocument()
})

test('more than twenty hosts: every one has its card, in a grid that scrolls', () => {
  render(Dashboard, props({ hosts: many(25) }))
  expect(cards()).toHaveLength(25)
  expect(names()[0]).toBe('node-01')
  expect(names()[24]).toBe('node-25')
  expect(screen.getByRole('region', { name: 'Hosts' })).toHaveClass('overflow-y-auto')
})

test('each card shows what is known about its host, and follows it', async () => {
  live['web-01'] = { ms: 12, peek }
  render(Dashboard, props())
  const card = screen.getByRole('article', { name: 'web-01' })
  expect(card).toHaveTextContent('CPU 23%')
  expect(screen.getByRole('article', { name: 'bare' })).toHaveTextContent('asking bare…')

  live['web-01'].peek = { ...peek, stats: { cpu: 97, mem: null, disk: null } }
  await vi.waitFor(() => expect(card).toHaveTextContent('CPU 97%'))
  expect(card).toHaveAttribute('data-state', 'crit')
})

test('a card connects to its host: by its button, or Enter on the card', async () => {
  const user = userEvent.setup()
  const onconnect = vi.fn()
  render(Dashboard, props({ onconnect }))
  const card = screen.getByRole('article', { name: 'db-primary' })
  await user.click(within(card).getByRole('button', { name: 'Connect' }))
  expect(onconnect).toHaveBeenLastCalledWith('db-primary')
  screen.getByRole('article', { name: 'web-02' }).focus()
  await user.keyboard('{Enter}')
  expect(onconnect).toHaveBeenLastCalledWith('web-02')
  expect(onconnect).toHaveBeenCalledTimes(2)
})

test('Space marks cards, as in the TUI; Enter then connects to every marked host and to no other', async () => {
  const user = userEvent.setup()
  const onconnect = vi.fn()
  render(Dashboard, props({ onconnect }))
  const card = (alias: string) => screen.getByRole('article', { name: alias })
  for (const alias of ['web-02', 'bare', 'db-primary', 'bare']) {
    card(alias).focus()
    await user.keyboard(' ')
  }
  // Marked twice is not marked.
  expect(within(card('web-02')).getByRole('img', { name: 'selected' })).toBeInTheDocument()
  expect(card('db-primary')).toHaveAttribute('data-marked')
  expect(card('bare')).not.toHaveAttribute('data-marked')
  expect(screen.getByRole('button', { name: 'Connect 2' })).toBeInTheDocument()

  // Wherever the focus is: the marked ones, in the order on screen.
  card('web-01').focus()
  await user.keyboard('{Enter}')
  const on_screen = screen.getAllByRole('article').map((el) => el.getAttribute('aria-label'))
  expect(onconnect.mock.calls.flat()).toEqual(on_screen.filter((alias) => ['web-02', 'db-primary'].includes(alias!)))
  // And the marks are spent: Enter is the card's own again.
  expect(screen.queryByRole('img', { name: 'selected' })).not.toBeInTheDocument()
  await user.keyboard('{Enter}')
  expect(onconnect).toHaveBeenLastCalledWith('web-01')
  expect(onconnect).toHaveBeenCalledTimes(3)

  // The button does the same for the mouse.
  await user.keyboard(' ')
  await user.click(screen.getByRole('button', { name: 'Connect 1' }))
  expect(onconnect).toHaveBeenCalledTimes(4)
  expect(screen.queryByRole('button', { name: /Connect \d/ })).not.toBeInTheDocument()
})

// --- M2: group, sort, filter, order -----------------------------------------

test('grouped by tag unless asked otherwise: a section per group with its count', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  // A host is in its group, else under its first tag, else under none.
  expect(headings()).toEqual(['prod (2)', 'db (1)', 'no tags (1)'])
  const prod = within(screen.getByRole('list', { name: 'prod' })).getAllByRole('article')
  expect(prod.map((card) => card.getAttribute('aria-label'))).toEqual(['web-01', 'web-02'])

  const toggle = screen.getByRole('button', { name: 'Group by tag' })
  expect(toggle).toHaveAttribute('aria-pressed', 'true')
  await user.click(toggle)
  expect(toggle).toHaveAttribute('aria-pressed', 'false')
  expect(headings()).toEqual([])
  expect(names()).toEqual(['web-01', 'web-02', 'db-primary', 'bare'])
})

test('sorted by name, by latency or by CPU; what is not known comes last', async () => {
  const user = userEvent.setup()
  live['web-01'] = { ms: 40, peek: cpu(10) }
  live['web-02'] = { ms: 5, peek: cpu(45) }
  live['db-primary'] = { ms: 12, peek: cpu(30) }
  render(Dashboard, props())
  await user.click(screen.getByRole('button', { name: 'Group by tag' }))

  expect(sort('My order')).toBeChecked()
  await user.click(sort('Name'))
  expect(names()).toEqual(['bare', 'db-primary', 'web-01', 'web-02'])
  await user.click(sort('Latency'))
  expect(names()).toEqual(['web-02', 'db-primary', 'web-01', 'bare'])
  await user.click(sort('CPU'))
  expect(names()).toEqual(['web-02', 'db-primary', 'web-01', 'bare'])
  // The grid follows the figures as they change.
  live['web-01'].peek = cpu(50)
  await vi.waitFor(() => expect(names()[0]).toBe('web-01'))
})

test('a sick host comes first, whatever the order, and its group with it', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  expect(names()).toEqual(['web-01', 'web-02', 'db-primary', 'bare'])

  live['db-primary'] = { error: errors[2] as ConnError }
  await vi.waitFor(() => expect(names()).toEqual(['db-primary', 'web-01', 'web-02', 'bare']))
  expect(headings()).toEqual(['db (1)', 'prod (2)', 'no tags (1)'])
  // Two of them: in the order they had, and each group is where its first card is.
  live['web-02'] = { peek: cpu(95) }
  await vi.waitFor(() => expect(names()).toEqual(['web-02', 'web-01', 'db-primary', 'bare']))

  // Flat and by name: the sick ones still lead, by name among themselves.
  await user.click(screen.getByRole('button', { name: 'Group by tag' }))
  await user.click(sort('Name'))
  expect(names()).toEqual(['db-primary', 'web-02', 'bare', 'web-01'])
  // Well again: back in line.
  live['db-primary'] = { ms: 3, peek: cpu(5) }
  await vi.waitFor(() => expect(names()).toEqual(['web-02', 'bare', 'db-primary', 'web-01']))
})

test('the tag chosen in the sidebar filters the grid', async () => {
  const { rerender } = render(Dashboard, props({ tag: 'eu' }))
  expect(names()).toEqual(['web-01', 'db-primary'])
  await rerender(props({ tag: 'PROD' }))
  expect(names()).toEqual(['web-01', 'web-02'])
  await rerender(props({ tag: null }))
  expect(names()).toHaveLength(4)
})

test('/ goes to the search box; it finds by name or address, and #tag by tag', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  const search = screen.getByRole('searchbox', { name: SEARCH })
  expect(search).not.toHaveFocus()
  await user.keyboard('/')
  expect(search).toHaveFocus()
  expect(search).toHaveValue('')

  await user.keyboard('WEB')
  expect(names()).toEqual(['web-01', 'web-02'])
  await user.clear(search)
  await user.keyboard('internal')
  expect(names()).toEqual(['db-primary'])
  await user.clear(search)
  // As `/#tag` in the TUI: the start of a tag or a group.
  await user.keyboard('#e')
  expect(names()).toEqual(['web-01', 'db-primary'])
  await user.clear(search)
  await user.keyboard('#pro')
  expect(names()).toEqual(['web-01', 'web-02'])

  await user.keyboard('zzz')
  expect(cards()).toHaveLength(0)
  expect(screen.getByText('No host matches the search.')).toBeInTheDocument()
  // Esc empties the box and gives the keys back to the grid.
  await user.keyboard('{Escape}')
  expect(search).toHaveValue('')
  expect(search).not.toHaveFocus()
  expect(names()).toHaveLength(4)
})

test('a / typed into the search box is a character, not a jump', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  await user.keyboard('/a/b')
  expect(screen.getByRole('searchbox')).toHaveValue('a/b')
})

test('a card dropped on another takes its place, and the order is remembered', async () => {
  const user = userEvent.setup()
  const { unmount } = render(Dashboard, props())
  await user.click(screen.getByRole('button', { name: 'Group by tag' }))
  const [web01, , , bare] = cards()
  expect(web01).toHaveAttribute('draggable', 'true')

  await fireEvent.dragStart(bare)
  // Over itself it would land where it is; over another, that one is lit until the pointer is off it.
  await fireEvent.dragOver(bare)
  expect(document.querySelector('[data-drop]')).toBeNull()
  await fireEvent.dragOver(web01)
  expect(web01).toHaveAttribute('data-drop')
  expect(web01).toHaveClass('cursor-grab', 'data-[drop]:border-blue')
  await fireEvent(web01, new MouseEvent('dragleave', { clientX: 5000, clientY: 5000, bubbles: true }))
  expect(document.querySelector('[data-drop]')).toBeNull()
  await fireEvent.dragOver(web01)
  await fireEvent.drop(web01)
  expect(document.querySelector('[data-drop]')).toBeNull()
  expect(names()).toEqual(['bare', 'web-01', 'web-02', 'db-primary'])
  // Down the list it lands after the card it was dropped on.
  await fireEvent.dragStart(screen.getByRole('article', { name: 'bare' }))
  await fireEvent.drop(screen.getByRole('article', { name: 'web-02' }))
  expect(names()).toEqual(['web-01', 'web-02', 'bare', 'db-primary'])
  expect(JSON.parse(localStorage.getItem('vssh.order')!)).toEqual(['web-01', 'web-02', 'bare', 'db-primary'])

  // A drop on itself, or with nothing picked up, changes nothing.
  await fireEvent.drop(screen.getByRole('article', { name: 'web-01' }))
  expect(names()).toEqual(['web-01', 'web-02', 'bare', 'db-primary'])

  unmount()
  render(Dashboard, props())
  await user.click(screen.getByRole('button', { name: 'Group by tag' }))
  expect(names()).toEqual(['web-01', 'web-02', 'bare', 'db-primary'])
})

test('the order survives hosts that came and went since it was saved', () => {
  localStorage.setItem('vssh.order', JSON.stringify(['gone', 'bare', 'web-02']))
  render(Dashboard, props({ hosts: [...hosts, ...many(1)] }))
  // Known ones in their saved order, then the rest as the config has them.
  expect(names().sort()).toEqual(['bare', 'db-primary', 'node-01', 'web-01', 'web-02'])
  const flat = within(screen.getByRole('list', { name: 'no tags' })).getAllByRole('article')
  expect(flat.map((card) => card.getAttribute('aria-label'))).toEqual(['bare', 'node-01'])
})

test('a saved order that is not one is ignored', () => {
  localStorage.setItem('vssh.order', '{"not": "a list"')
  render(Dashboard, props())
  expect(names()).toEqual(['web-01', 'web-02', 'db-primary', 'bare'])
})

test('dragging in a sorted grid keeps what is on screen and moves only the card', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  await user.click(screen.getByRole('button', { name: 'Group by tag' }))
  await user.click(sort('Name'))
  expect(names()).toEqual(['bare', 'db-primary', 'web-01', 'web-02'])
  await fireEvent.dragStart(screen.getByRole('article', { name: 'web-02' }))
  await fireEvent.drop(screen.getByRole('article', { name: 'bare' }))
  expect(names()).toEqual(['web-02', 'bare', 'db-primary', 'web-01'])
  expect(sort('My order')).toBeChecked()
})

test('without a mouse: Alt and an arrow move the focused card, which keeps the focus', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  await user.click(screen.getByRole('button', { name: 'Group by tag' }))
  screen.getByRole('article', { name: 'web-02' }).focus()
  await user.keyboard('{Alt>}{ArrowRight}{/Alt}')
  expect(names()).toEqual(['web-01', 'db-primary', 'web-02', 'bare'])
  await vi.waitFor(() => expect(screen.getByRole('article', { name: 'web-02' })).toHaveFocus())
  await user.keyboard('{Alt>}{ArrowLeft}{ArrowLeft}{/Alt}')
  expect(names()).toEqual(['web-02', 'web-01', 'db-primary', 'bare'])
  // At the edge there is nowhere to go.
  await user.keyboard('{Alt>}{ArrowLeft}{/Alt}')
  expect(names()).toEqual(['web-02', 'web-01', 'db-primary', 'bare'])
  expect(localStorage.getItem('vssh.order')).toContain('web-02')
})

test('the selected host has its card marked and focused; a focused card is the selected one', async () => {
  const onselect = vi.fn()
  const { rerender } = render(Dashboard, props({ selected: null, onselect }))
  await rerender(props({ selected: 'db-primary', onselect }))
  const card = screen.getByRole('article', { name: 'db-primary' })
  await vi.waitFor(() => expect(card).toHaveFocus())
  expect(card).toHaveAttribute('aria-current', 'true')
  screen.getByRole('article', { name: 'bare' }).focus()
  expect(onselect).toHaveBeenLastCalledWith('bare')
})

test('x, s and L ask for a dialog and are not typed into what it opens with', async () => {
  const heard = { onbatch: vi.fn(), onsnippets: vi.fn(), onforwards: vi.fn() }
  render(Dashboard, props(heard))
  const card = screen.getByRole('article', { name: 'web-01' })
  card.focus()
  // In the webview the dialog's field has the focus before the key is done:
  // a key that is not stopped is the first letter in it.
  for (const key of ['x', 's', 'L']) expect(await fireEvent.keyDown(card, { key }), key).toBe(false)
  expect(heard.onbatch).toHaveBeenCalledOnce()
  expect(heard.onsnippets).toHaveBeenCalledOnce()
  expect(heard.onforwards).toHaveBeenCalledExactlyOnceWith('web-01')
  // A key that is nobody's is left alone.
  expect(await fireEvent.keyDown(card, { key: 'q' })).toBe(true)
})

test('the bar over the cards is two rows from one margin: the search with the new host beside it, and under it the tags and how the cards are laid out', async () => {
  const user = userEvent.setup()
  render(Dashboard, props({ onadd: vi.fn() }))
  cards()[0].focus()
  await user.keyboard(' ')
  const search = screen.getByRole('searchbox', { name: SEARCH })
  const [first, second] = [search.parentElement!, search.parentElement!.nextElementSibling!]
  const inside = (row: Element) => [...row.querySelectorAll(':scope > *')].map((el) => el.getAttribute('aria-label') ?? el.textContent?.trim())
  // In the order of the Tab key: the search as wide as there is room, then the one filled button.
  expect(inside(first)).toEqual([SEARCH, 'Add host'])
  expect(search).toHaveClass('flex-1')
  expect(inside(second)).toEqual(['Tags', 'Group by tag', 'Connect 1', 'Run 1', 'Sort'])
  expect(second.nextElementSibling).toBeNull()
  // The bar stays over the cards; its second row breaks only where even that is too narrow.
  expect(first.parentElement).toHaveClass('grid', 'grid-cols-1', 'sticky', 'bg-bg')
  expect(second).toHaveClass('flex', 'flex-wrap')
})

test('the bar has the hosts as a JSON file, out and in, before the one filled button; not while an address is typed', async () => {
  const user = userEvent.setup()
  const heard = { onadd: vi.fn(), onexport: vi.fn(), onimport: vi.fn(), onquick: vi.fn() }
  render(Dashboard, props(heard))
  const search = screen.getByRole('searchbox', { name: SEARCH })
  const row = () => [...search.parentElement!.querySelectorAll(':scope > *')].map((el) => el.getAttribute('aria-label') ?? el.textContent?.trim())
  expect(row()).toEqual([SEARCH, 'Export hosts (JSON)', 'Import hosts (JSON)', 'Add host'])
  expect(search.parentElement).toHaveClass('flex-wrap')
  await user.click(screen.getByRole('button', { name: 'Export hosts (JSON)' }))
  expect(heard.onexport).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('button', { name: 'Import hosts (JSON)' }))
  expect(heard.onimport).toHaveBeenCalledOnce()
  // The row is the connection's while one is typed.
  await user.type(search, 'root@10.1.1.1')
  expect(row()).toEqual([SEARCH, 'Connect to root@10.1.1.1', 'Add host'])
  setLang('vi')
  await user.clear(search)
  expect(screen.getByRole('button', { name: 'Xuất máy chủ (JSON)' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Nhập máy chủ (JSON)' })).toBeInTheDocument()
})

// --- D9: the concept's dashboard ------------------------------------------------

test('the cards are in one column, two past 800 px of window, three past 1200', () => {
  render(Dashboard, props())
  for (const list of screen.getAllByRole('list').filter((list) => list.querySelector('article'))) {
    expect(list).toHaveClass('grid', 'grid-cols-1', '@min-[31rem]:grid-cols-2', '@min-[56rem]:grid-cols-3')
  }
})

test('the figures over the cards: how many answer, the hosts\' CPU and memory as one line each, and who is down', () => {
  live['web-01'] = { ms: 12, peek }
  live['web-02'] = { ms: 12, peek: cpu(95) }
  live['db-primary'] = { ms: null, error: errors[2] as ConnError }
  trail['web-01'] = { cpu: [10, 20], mem: [40] }
  trail['web-02'] = { cpu: [90], mem: [] }
  const { container } = render(Dashboard, props())
  const strip = container.querySelector('[aria-hidden=true].grid-cols-4')!
  const tiles = [...strip.children].map((tile) => tile.textContent?.replace(/\s+/g, ' ').trim())
  // Two answer (one of them past a critical mark), of four; the mean of the newest round is (20 + 90) / 2.
  expect(tiles).toEqual(['reachable 2 / 4', 'CPU 55%', 'MEM 40%', 'down 1 db-primary'])
  // A segment for every host, in the colour of its state.
  expect([...strip.querySelectorAll('i')].map((part) => part.className.match(/bg-green|bg-red|inset-ring-red|bg-bar/)?.[0])).toEqual(['bg-green', 'bg-red', 'inset-ring-red', 'bg-bar'])
  // The older round has one host's figure only: that is its mean.
  expect(strip.querySelector('.text-blue path[stroke]')).toHaveAttribute('d', 'M0,27.2 C60,27.2 60,14.6 120,14.6')
})

test('before anything is known the figures are dashes, not noughts', () => {
  const { container } = render(Dashboard, props())
  const strip = container.querySelector('[aria-hidden=true].grid-cols-4')!
  expect(strip).toHaveTextContent('reachable 0 / 4 CPU — MEM — down 0')
  expect(strip.querySelectorAll('path')).toHaveLength(0)
})

test('Try again on a card that is down asks its host, as a person asking', async () => {
  const user = userEvent.setup()
  const asked: unknown[] = []
  mockIPC((cmd, args) => {
    if (cmd === 'get_host_stats') asked.push(args)
    return cmd === 'get_host_stats' ? peek : null
  })
  live['web-02'] = { ms: null, error: errors[2] as ConnError, shown: { class: 'refused', label: 'REFUSED', message: 'no', chain: 'no' } }
  render(Dashboard, props())
  await user.click(within(screen.getByRole('article', { name: 'web-02' })).getByRole('button', { name: 'try again' }))
  expect(asked).toEqual([{ alias: 'web-02', passive: false }])
  await vi.waitFor(() => expect(screen.getByRole('article', { name: 'web-02' })).toHaveAttribute('data-state', 'ok'))
  clearMocks()
})

test('a host nobody asked: its card\'s Quick peek button asks it, as v does; the toolbar\'s buttons name their keys', async () => {
  const asked: unknown[] = []
  mockIPC((cmd, args) => {
    asked.push([cmd, args])
    return null
  })
  live.bare = { peek: null }
  const user = userEvent.setup()
  render(Dashboard, props({ onadd: vi.fn(), onbatch: vi.fn() }))
  await user.click(within(screen.getByRole('article', { name: 'bare' })).getByRole('button', { name: 'Quick peek' }))
  expect(screen.getByRole('dialog', { name: 'Quick peek: bare' })).toBeInTheDocument()
  expect(asked).toEqual([['get_host_stats', { alias: 'bare', passive: false }]])
  expect(screen.getByRole('button', { name: 'Add host' })).toHaveAttribute('title', 'Add host (A)')
  expect(screen.getByRole('searchbox')).toHaveAttribute('title', `${SEARCH} (/, Ctrl+F)`)
  clearMocks()
})

test('with Ctrl a card\'s letters are not its keys: Ctrl+F on a card is the search and opens no files', async () => {
  const user = userEvent.setup()
  const on = { onfiles: vi.fn(), onforwards: vi.fn(), onedit: vi.fn(), ondelete: vi.fn(), onconnect: vi.fn() }
  render(Dashboard, props(on))
  screen.getByRole('article', { name: 'web-01' }).focus()
  await user.keyboard('{Control>}f{/Control}')
  expect(screen.getByRole('searchbox')).toHaveFocus()
  expect(on.onfiles).not.toHaveBeenCalled()
  // Nor the others: Ctrl+L, Ctrl+E, Ctrl+D and Ctrl+Enter are nobody's here.
  screen.getByRole('article', { name: 'web-01' }).focus()
  await user.keyboard('{Control>}led{Enter}{/Control}')
  for (const asked of Object.values(on)) expect(asked).not.toHaveBeenCalled()
  // Without it they are the card's, as before.
  await user.keyboard('f')
  expect(on.onfiles).toHaveBeenCalledWith('web-01')
})

test('the right button on a card: what its keys do, as a menu, and its address for the clipboard; a double click connects', async () => {
  const user = userEvent.setup()
  const copied: unknown[] = []
  mockIPC((cmd, args) => {
    if (cmd === 'plugin:clipboard-manager|write_text') copied.push((args as { text: string }).text)
    return null
  })
  const on = { onconnect: vi.fn(), onfiles: vi.fn(), onforwards: vi.fn(), onedit: vi.fn(), ondelete: vi.fn(), onpalette: vi.fn(), onselect: vi.fn() }
  render(Dashboard, props(on))
  const card = screen.getByRole('article', { name: 'web-01' })
  const choose = async (name: string | RegExp) => {
    await fireEvent.contextMenu(card, { clientX: 300, clientY: 200 })
    await user.click(within(screen.getByRole('menu', { name: 'web-01' })).getByRole('menuitem', { name }))
    expect(screen.queryByRole('menu')).toBeNull()
  }
  await fireEvent.contextMenu(card, { clientX: 300, clientY: 200 })
  // The card the menu is about is the selected one.
  expect(on.onselect).toHaveBeenLastCalledWith('web-01')
  expect(within(screen.getByRole('menu')).getAllByRole('menuitem').map((item) => item.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
    'Connect Enter',
    'Files F',
    'Forwards L',
    'Edit E',
    'Copy address',
    'Command palette Ctrl+K',
    'Delete D',
  ])
  // The arrows go through it, round the end; Esc gives the keys back to the card.
  await user.keyboard('{ArrowUp}')
  expect(screen.getByRole('menuitem', { name: /Delete/ })).toHaveFocus()
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('menu')).toBeNull()
  expect(card).toHaveFocus()
  await choose(/Connect/)
  expect(on.onconnect).toHaveBeenLastCalledWith('web-01')
  await choose('Copy address')
  expect(copied).toEqual(['deploy@10.0.0.5:2222'])
  await choose(/Command palette/)
  expect(on.onpalette).toHaveBeenCalledOnce()
  await choose(/Delete/)
  expect(on.ondelete).toHaveBeenLastCalledWith('web-01')
  await choose(/Files/)
  expect(on.onfiles).toHaveBeenLastCalledWith('web-01')
  // A double click on the card connects; on its own button that is the button's two clicks.
  on.onconnect.mockClear()
  await user.dblClick(within(card).getByRole('heading', { name: 'web-01' }))
  expect(on.onconnect).toHaveBeenCalledTimes(1)
  await user.dblClick(within(card).getByRole('button', { name: 'Connect' }))
  expect(on.onconnect).toHaveBeenCalledTimes(3)
  clearMocks()
})

// --- D11: the well are low, the sick are tall -----------------------------------

test('the hosts that need looking at are tall and come first; the first that is well starts a row of its own, of low cards', () => {
  live['web-01'] = { ms: 12, peek }
  live['web-02'] = { ms: 12, peek: cpu(100) }
  live['db-primary'] = { ms: null, error: errors[2] as ConnError }
  live['bare'] = { ms: 3, peek: null }
  render(Dashboard, props())
  // Flat: one list of them all.
  fireEvent.click(screen.getByRole('button', { name: 'Group by tag' }))
  const low = () => cards().map((card) => [card.getAttribute('aria-label'), card.hasAttribute('data-low'), card.parentElement!.classList.contains('col-start-1')])
  return vi.waitFor(() =>
    expect(low()).toEqual([
      ['web-02', false, false],
      ['db-primary', false, false],
      ['web-01', true, true],
      ['bare', true, false],
    ]),
  )
})

// --- D12: the hosts are here, by group --------------------------------------------

test('a section a group, named as "group (how many)", each a grid of its cards; a group the search leaves empty is not shown', async () => {
  const user = userEvent.setup()
  const fleet: Host[] = [
    ...hosts,
    { ...hosts[3], alias: 'cache-01', meta: { ...hosts[3].meta, grp: 'staging', tags: 'cache' } },
    { ...hosts[3], alias: 'cache-02', meta: { ...hosts[3].meta, grp: 'staging' } },
  ]
  render(Dashboard, props({ hosts: fleet }))
  expect(headings()).toEqual(['prod (2)', 'db (1)', 'no tags (1)', 'staging (2)'])
  for (const [group, inside] of [['prod', ['web-01', 'web-02']], ['staging', ['cache-01', 'cache-02']], ['no tags', ['bare']]] as const) {
    const grid = screen.getByRole('list', { name: group })
    expect(grid, group).toHaveClass('grid', '@min-[31rem]:grid-cols-2', '@min-[56rem]:grid-cols-3')
    expect(within(grid).getAllByRole('article').map((card) => card.getAttribute('aria-label'))).toEqual(inside)
  }
  await user.keyboard('/cache')
  expect(headings()).toEqual(['staging (2)'])
  expect(screen.queryByRole('list', { name: 'prod' })).toBeNull()
  await user.keyboard('{Escape}/zzz')
  expect(headings()).toEqual([])
  expect(screen.getByText('No host matches the search.')).toBeInTheDocument()
})

test('a card says who its host is: its name, user@host under it, and its tags', () => {
  live['web-01'] = { ms: 12, peek }
  live['bare'] = { ms: 3, peek: null }
  render(Dashboard, props())
  const card = screen.getByRole('article', { name: 'web-01' })
  expect(within(card).getByRole('heading', { name: 'web-01' })).toBeInTheDocument()
  expect(within(card).getByText('deploy@10.0.0.5:2222')).toBeInTheDocument()
  expect(within(within(card).getByRole('list', { name: 'Tags' })).getAllByRole('listitem').map((tag) => tag.textContent)).toEqual(['web', 'eu', 'prod'])
  // A host with nothing but a name: nothing made up under it.
  const bare = screen.getByRole('article', { name: 'bare' })
  expect(bare.textContent).not.toMatch(/null|undefined|@/)
  expect(within(bare).queryByRole('list', { name: 'Tags' })).toBeNull()
})

const chips = () => within(screen.getAllByRole('list', { name: 'Tags' })[0]).getAllByRole('button')

test('the tags are one row under the search, however many: a click narrows the cards to one, a second lets go, and so does All', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  const row = screen.getAllByRole('list', { name: 'Tags' })[0]
  // One row: it does not wrap, and what has no room is scrolled to.
  expect(row).toHaveClass('flex', 'overflow-x-auto')
  expect(row).not.toHaveClass('flex-wrap')
  for (const item of within(row).getAllByRole('listitem')) expect(item).toHaveClass('shrink-0')
  // Most used first, as the rail has them; short: a name and no number.
  expect(chips().map((chip) => chip.textContent?.trim())).toEqual(['All', '#eu', '#prod', '#web', '#db', '#us'])
  expect(chips()[0]).toHaveAttribute('aria-pressed', 'true')

  await user.click(screen.getByRole('button', { name: '#db' }))
  expect(names()).toEqual(['db-primary'])
  expect(screen.getByRole('button', { name: '#db' })).toHaveAttribute('aria-pressed', 'true')
  expect(chips()[0]).toHaveAttribute('aria-pressed', 'false')
  await user.click(screen.getByRole('button', { name: '#db' }))
  expect(names()).toHaveLength(4)
  await user.click(screen.getByRole('button', { name: '#prod' }))
  expect(names()).toEqual(['web-01', 'web-02'])
  await user.click(screen.getByRole('button', { name: 'All' }))
  expect(names()).toHaveLength(4)
})

test('hosts with no tags at all: no row of them, and no All for nothing', () => {
  render(Dashboard, props({ hosts: many(3) }))
  expect(screen.queryByRole('button', { name: 'All' })).toBeNull()
})

test('the search is also the quick connect: user@hostname typed there is connected to as it stands, by its button or by Enter when no host is found', async () => {
  const user = userEvent.setup()
  const onquick = vi.fn(async () => {})
  render(Dashboard, props({ onquick }))
  const search = screen.getByRole('searchbox', { name: SEARCH })
  expect(search).toHaveAttribute('placeholder', SEARCH)
  const connect = () => screen.queryByRole('button', { name: /^Connect to/ })
  // The name of a host: its card, and nothing to connect to by hand.
  await user.type(search, 'web')
  expect(connect()).toBeNull()
  await user.clear(search)
  await user.type(search, 'root@10.9.9.9:2200')
  expect(names()).toEqual([])
  expect(connect()).toHaveAccessibleName('Connect to root@10.9.9.9:2200')
  expect(connect()).toHaveAttribute('title', 'Quick connect · Nothing is saved.')
  await user.keyboard('{Enter}')
  expect(onquick).toHaveBeenCalledExactlyOnceWith('root@10.9.9.9:2200')
  // Connected: the search is empty again, and the hosts are back for the next time.
  await vi.waitFor(() => expect(search).toHaveValue(''))
  expect(names()).toHaveLength(4)

  // A bare name that is no host's, by the button.
  await user.type(search, 'sandbox.example')
  await user.click(connect()!)
  expect(onquick).toHaveBeenLastCalledWith('sandbox.example')
  // A tag is no address, nor are two words.
  await vi.waitFor(() => expect(search).toHaveValue(''))
  await user.type(search, '#zzz')
  expect(connect()).toBeNull()
  await user.clear(search)
  await user.type(search, 'no such')
  expect(connect()).toBeNull()
  await user.keyboard('{Enter}')
  expect(onquick).toHaveBeenCalledTimes(2)
})

test('an address that finds a host too: Enter and the arrow go to the card found, and the button is the way to the address itself', async () => {
  const user = userEvent.setup()
  const onquick = vi.fn(async () => {})
  render(Dashboard, props({ onquick }))
  const search = screen.getByRole('searchbox', { name: SEARCH })
  await user.type(search, 'deploy@10.0.0')
  expect(names()).toEqual(['web-01', 'web-02'])
  expect(screen.getByRole('button', { name: 'Connect to deploy@10.0.0' })).toBeInTheDocument()
  await user.keyboard('{Enter}')
  expect(onquick).not.toHaveBeenCalled()
  expect(screen.getByRole('article', { name: 'web-01' })).toHaveFocus()
  search.focus()
  await user.keyboard('{ArrowDown}')
  expect(screen.getByRole('article', { name: 'web-01' })).toHaveFocus()
})

test('a quick connect that is refused says why under the search, and what was typed stays to be mended; typing takes the refusal away', async () => {
  const user = userEvent.setup()
  const onquick = vi.fn(async () => {
    throw { label: 'DNS', message: 'Could not resolve nowhere.example' }
  })
  render(Dashboard, props({ onquick }))
  const search = screen.getByRole('searchbox', { name: SEARCH })
  await user.type(search, 'nowhere.example{Enter}')
  expect(await screen.findByRole('alert')).toHaveTextContent('DNS Could not resolve nowhere.example')
  expect(search).toHaveValue('nowhere.example')
  await user.type(search, 's')
  expect(screen.queryByRole('alert')).toBeNull()
})

test('with no way to connect by hand the search only searches', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  await user.type(screen.getByRole('searchbox', { name: SEARCH }), 'root@10.9.9.9')
  expect(screen.queryByRole('button', { name: /^Connect to/ })).toBeNull()
})
