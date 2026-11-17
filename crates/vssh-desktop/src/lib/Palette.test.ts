import { render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import type { Host } from './api'
import fixture from './fixtures/hosts.json'
import { setLang } from './i18n.svelte'
import Palette from './Palette.svelte'

const hosts: Host[] = fixture

function open(over = {}) {
  const props = { hosts, onhost: vi.fn(), onquick: vi.fn(), onsnippets: vi.fn(), onclose: vi.fn(), ...over }
  render(Palette, props)
  return { ...props, user: userEvent.setup(), input: screen.getByRole('combobox') }
}
/** The label of every option: the alias, or the action's name. */
const options = () => screen.getAllByRole('option').map((o) => o.firstElementChild?.textContent)
const chosen = () => screen.getByRole('option', { selected: true })

test('with nothing typed: the actions, then every host', () => {
  open()
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
  expect(options()).toEqual(['Quick connect', 'Snippets', 'web-01', 'web-02', 'db-primary', 'bare'])
  expect(chosen()).toHaveTextContent('Quick connect')
})

test('"snip" finds the snippets, in either language', async () => {
  const { user, input, onsnippets, onhost } = open()
  await user.type(input, 'snip')
  expect(options()).toEqual(['Snippets'])
  await user.keyboard('{Enter}')
  expect(onsnippets).toHaveBeenCalledOnce()
  expect(onhost).not.toHaveBeenCalled()
  setLang('vi')
  expect(await screen.findByRole('option', { name: /Đoạn lệnh/ })).toBeInTheDocument()
  await user.clear(input)
  await user.type(input, 'doan')
  expect(chosen()).toHaveTextContent('Đoạn lệnh')
})

test('typing filters the hosts, best match first', async () => {
  const { user, input } = open()
  await user.type(input, 'web')
  expect(options()).toEqual(['web-01', 'web-02'])
  await user.clear(input)
  await user.type(input, 'db')
  expect(chosen()).toHaveTextContent('db-primary')
  expect(screen.queryByText('Quick connect')).not.toBeInTheDocument()
})

test('a host is found by its user, its address and its tags too', async () => {
  const { user, input } = open()
  await user.type(input, 'postgres')
  expect(options()).toEqual(['db-primary'])
  await user.clear(input)
  await user.type(input, '10.0.0.6')
  expect(options()).toEqual(['web-02'])
  await user.clear(input)
  await user.type(input, 'prod')
  expect(options()).toEqual(['web-01', 'web-02'])
})

test('enter chooses the marked host; the arrows move the mark', async () => {
  const { user, input, onhost, onquick } = open()
  await user.type(input, 'web')
  expect(input).toHaveAttribute('aria-activedescendant', chosen().id)
  await user.keyboard('{ArrowDown}')
  expect(chosen()).toHaveTextContent('web-02')
  // Past the last one is the first again, and back.
  await user.keyboard('{ArrowDown}')
  expect(chosen()).toHaveTextContent('web-01')
  await user.keyboard('{ArrowUp}{Enter}')
  expect(onhost).toHaveBeenCalledExactlyOnceWith('web-02')
  expect(onquick).not.toHaveBeenCalled()
})

test('"qc" finds the quick connect action, in either language', async () => {
  const { user, input, onquick, onhost } = open()
  await user.type(input, 'qc')
  expect(options()).toEqual(['Quick connect'])
  await user.keyboard('{Enter}')
  expect(onquick).toHaveBeenCalledOnce()
  expect(onhost).not.toHaveBeenCalled()

  setLang('vi')
  await user.clear(input)
  await user.type(input, 'ket noi')
  expect(chosen()).toHaveTextContent('Kết nối nhanh')
  await user.clear(input)
  await user.type(input, 'qc')
  expect(chosen()).toHaveTextContent('Kết nối nhanh')
})

test('a click chooses too', async () => {
  const { user, onhost } = open()
  await user.click(screen.getByRole('option', { name: /db-primary/ }))
  expect(onhost).toHaveBeenCalledExactlyOnceWith('db-primary')
})

test('nothing matches: it says so, and enter does nothing', async () => {
  const { user, input, onhost, onquick } = open()
  await user.type(input, 'zzzz{Enter}')
  expect(screen.queryAllByRole('option')).toHaveLength(0)
  expect(screen.getByText('No host matches the search.')).toBeInTheDocument()
  expect(input).not.toHaveAttribute('aria-activedescendant')
  expect(onhost).not.toHaveBeenCalled()
  expect(onquick).not.toHaveBeenCalled()
})

test('with no hosts at all the actions are still there', () => {
  open({ hosts: [] })
  expect(options()).toEqual(['Quick connect', 'Snippets'])
})

test('closing the dialog tells the app', () => {
  const { onclose } = open()
  screen.getByRole<HTMLDialogElement>('dialog').close()
  expect(onclose).toHaveBeenCalledOnce()
})
