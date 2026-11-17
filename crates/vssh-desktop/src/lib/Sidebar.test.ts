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

test('in Vietnamese the words are the TUI\'s', () => {
  setLang('vi')
  render(Sidebar, props({ hosts: [] }))
  expect(screen.getByRole('heading', { name: 'Máy chủ 0' })).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: 'Thẻ 0' })).toBeInTheDocument()
  expect(screen.getByText('chưa có thẻ')).toBeInTheDocument()
})
