import { fireEvent, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { ConnError, Host, Peek } from './api'
import Dashboard from './Dashboard.svelte'
import errors from './fixtures/errors.json'
import fixture from './fixtures/hosts.json'
import peekFixture from './fixtures/peek.json'
import { forget, live } from './live.svelte'

const hosts = fixture as Host[]
const peek = peekFixture as unknown as Peek
const props = (over = {}) => ({ hosts, onconnect: vi.fn(), onpalette: vi.fn(), ...over })
const cards = () => screen.queryAllByRole('article')
const names = () => cards().map((card) => card.getAttribute('aria-label'))
/** `n` hosts with nothing but a name. */
const many = (n: number): Host[] =>
  Array.from({ length: n }, (_, i) => ({ ...hosts[3], alias: `node-${String(i + 1).padStart(2, '0')}` }))

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

// --- M2: group, sort, filter, order -----------------------------------------

test('grouped by tag unless asked otherwise: a section per group with its count', async () => {
  const user = userEvent.setup()
  render(Dashboard, props())
  // A host is in its group, else under its first tag, else under none.
  expect(headings()).toEqual(['#prod 2', '#db 1', 'no tags 1'])
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
  expect(headings()).toEqual(['#db 1', '#prod 2', 'no tags 1'])
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
  const search = screen.getByRole('searchbox', { name: 'Search hosts, #tag' })
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
  await fireEvent.dragOver(web01)
  await fireEvent.drop(web01)
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
