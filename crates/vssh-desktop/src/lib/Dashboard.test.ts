import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import type { Host, Peek } from './api'
import Dashboard from './Dashboard.svelte'
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

afterEach(forget)

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
