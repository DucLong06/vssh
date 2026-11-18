import { render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import type { Host } from './api'
import fixture from './fixtures/hosts.json'
import { setLang } from './i18n.svelte'
import { setStreamer } from './streamer.svelte'
import Palette from './Palette.svelte'

const hosts: Host[] = fixture

function open<Over extends object>(over = {} as Over) {
  const props = { hosts, onhost: vi.fn(), onconnect: vi.fn(async () => {}), onsnippets: vi.fn(), onclose: vi.fn(), ...over }
  render(Palette, props)
  return { ...props, user: userEvent.setup(), input: screen.getByRole('combobox') }
}
/** The label of every option: the alias, or the action's name. */
const options = () => screen.getAllByRole('option').map((o) => o.firstElementChild?.textContent)
const chosen = () => screen.getByRole('option', { selected: true })

test('with nothing typed: the actions, then every host', () => {
  open()
  expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument()
  expect(options()).toEqual(['Snippets', 'web-01', 'web-02', 'db-primary', 'bare'])
  expect(chosen()).toHaveTextContent('Snippets')
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
  expect(screen.queryByText(/Connect to/)).not.toBeInTheDocument()
})

test('a host is found by its user, its address and its tags too', async () => {
  const { user, input } = open()
  await user.type(input, 'postgres')
  expect(options()).toEqual(['db-primary'])
  await user.clear(input)
  await user.type(input, '10.0.0.6')
  expect(options()).toEqual(['web-02', 'Connect to 10.0.0.6'])
  await user.clear(input)
  await user.type(input, 'prod')
  expect(options()).toEqual(['web-01', 'web-02'])
})

test('enter chooses the marked host; the arrows move the mark', async () => {
  const { user, input, onhost, onconnect } = open()
  await user.type(input, 'web')
  expect(input).toHaveAttribute('aria-activedescendant', chosen().id)
  await user.keyboard('{ArrowDown}')
  expect(chosen()).toHaveTextContent('web-02')
  // Past the last one is the first again, and back.
  await user.keyboard('{ArrowDown}')
  expect(chosen()).toHaveTextContent('web-01')
  await user.keyboard('{ArrowUp}{Enter}')
  expect(onhost).toHaveBeenCalledExactlyOnceWith('web-02')
  expect(onconnect).not.toHaveBeenCalled()
})

test('what is typed and is the name of nothing here is an address: the one row connects to it as it stands, and saves nothing', async () => {
  const { user, input, onconnect, onhost } = open()
  await user.type(input, 'deploy@10.0.0.9:2200')
  expect(options()).toEqual(['Connect to deploy@10.0.0.9:2200'])
  expect(chosen()).toHaveTextContent('Quick connect · Nothing is saved.')
  expect([...chosen().querySelectorAll('kbd')].map((key) => key.textContent)).toEqual(['Enter'])
  await user.keyboard('{Enter}')
  expect(onconnect).toHaveBeenCalledExactlyOnceWith('deploy@10.0.0.9:2200')
  expect(onhost).not.toHaveBeenCalled()
  // A host's address finds the host first; the address as it stands comes after, for another user or port there.
  await user.clear(input)
  await user.type(input, '10.0.0.6')
  expect(options()).toEqual(['web-02', 'Connect to 10.0.0.6'])
  expect(chosen()).toHaveTextContent('web-02')
  // A name that finds something is no address.
  await user.clear(input)
  await user.type(input, 'web')
  expect(options()).toEqual(['web-01', 'web-02'])
  setLang('vi')
  await user.clear(input)
  await user.type(input, 'new-box')
  expect(options()).toEqual(['Kết nối tới new-box'])
  expect(chosen()).toHaveTextContent('Kết nối nhanh · Không có gì được lưu.')
})

test('an address that refuses: why is said under the list, in the palette, until something else is typed', async () => {
  const refusal = { class: 'refused', label: 'REFUSED', message: '10.0.0.9:22 refused the connection', chain: '' }
  const { user, input, onclose } = open({ onconnect: vi.fn(() => Promise.reject(refusal)) })
  await user.type(input, '10.0.0.9{Enter}')
  expect(await screen.findByRole('alert')).toHaveTextContent('REFUSED 10.0.0.9:22 refused the connection')
  expect(onclose).not.toHaveBeenCalled()
  await user.type(input, '1')
  expect(screen.queryByRole('alert')).toBeNull()
})

test('in streamer mode an address being typed is dots in the field, and a made-up one in its row', async () => {
  setStreamer(true)
  const { user, input } = open()
  await user.type(input, 'web')
  expect(input).not.toHaveAttribute('data-masked')
  await user.clear(input)
  await user.type(input, 'root@203.0.113.7')
  expect(input).toHaveAttribute('data-masked')
  expect(input).toHaveClass('data-[masked]:[-webkit-text-security:disc]')
  expect(options()[0]).toMatch(/^Connect to root@/)
  expect(options()[0]).not.toContain('203.0.113.7')
  setStreamer(false)
})

test('a click chooses too', async () => {
  const { user, onhost } = open()
  await user.click(screen.getByRole('option', { name: /db-primary/ }))
  expect(onhost).toHaveBeenCalledExactlyOnceWith('db-primary')
})

test('nothing matches, and words are no address: it says so, and enter does nothing', async () => {
  const { user, input, onhost, onconnect } = open()
  await user.type(input, 'zz zz{Enter}')
  expect(screen.queryAllByRole('option')).toHaveLength(0)
  expect(screen.getByText('No host matches the search.')).toBeInTheDocument()
  expect(input).not.toHaveAttribute('aria-activedescendant')
  expect(onhost).not.toHaveBeenCalled()
  expect(onconnect).not.toHaveBeenCalled()
})

test('with no hosts at all the actions are still there', () => {
  open({ hosts: [] })
  expect(options()).toEqual(['Snippets'])
})

test('closing the dialog tells the app', () => {
  const { onclose } = open()
  screen.getByRole<HTMLDialogElement>('dialog').close()
  expect(onclose).toHaveBeenCalledOnce()
})

test('the vault and a new host are actions too, where the window has them', async () => {
  const { user, input, onvault, onadd } = open({ onvault: vi.fn(), onadd: vi.fn() })
  expect(options().slice(0, 3)).toEqual(['Snippets', 'Add host', 'Vault'])
  await user.type(input, 'vault{Enter}')
  expect(onvault).toHaveBeenCalledOnce()
  await user.clear(input)
  await user.type(input, 'add host{Enter}')
  expect(onadd).toHaveBeenCalledOnce()
  setLang('vi')
  expect(await screen.findByRole('option', { name: /Thêm máy chủ/ })).toBeInTheDocument()
})

test('an action shows its own key as a key cap, and the cheatsheet is an action; the foot names the keys as a keyboard has them', async () => {
  const { user, input, onshortcuts } = open({ onadd: vi.fn(), onshortcuts: vi.fn() })
  const caps = (name: RegExp) => [...screen.getByRole('option', { name }).querySelectorAll('kbd')].map((key) => key.textContent)
  expect(caps(/Snippets/)).toEqual(['S'])
  expect(caps(/Add host/)).toEqual(['A'])
  expect(caps(/Keyboard shortcuts/)).toEqual(['?', 'F1', 'Ctrl+/'])
  // A host has no key of its own.
  expect(caps(/db-primary/)).toEqual([])
  expect(screen.getByRole('contentinfo')).toHaveTextContent('↑ ↓ Enter select Esc close')
  await user.type(input, 'keyb{Enter}')
  expect(onshortcuts).toHaveBeenCalledOnce()
  setLang('vi')
  await user.clear(input)
  await user.type(input, 'phim tat')
  expect(chosen()).toHaveTextContent('Phím tắt')
  setLang('en')
})

test('the window\'s own shape is in the palette: focus mode, and a split while a tab is in front; a host\'s name finds none of them', async () => {
  const { user, input, onzen, onsplit } = open({ onzen: vi.fn(), onsplit: vi.fn() })
  expect(options().slice(0, 4)).toEqual(['Snippets', 'Split right', 'Split down', 'Focus mode'])
  const keys = (name: RegExp) => [...screen.getByRole('option', { name }).querySelectorAll('kbd')].map((key) => key.textContent)
  expect(keys(/Split right/)).toEqual(['Ctrl+Shift+D'])
  expect(keys(/Split down/)).toEqual(['Ctrl+Shift+E'])
  expect(keys(/Focus mode/)).toEqual(['Ctrl+Shift+B', 'Ctrl+B'])
  // Two letters of a host's name are not scattered over an action's: db is the two hosts it is in.
  for (const lang of ['en', 'vi'] as const) {
    setLang(lang)
    await user.clear(input)
    await user.type(input, 'db')
    expect(options(), lang).toEqual(['db-primary'])
    await user.clear(input)
    await user.type(input, 'web')
    expect(options(), lang).toEqual(['web-01', 'web-02'])
  }
  await user.clear(input)
  await user.type(input, 'split down{Enter}')
  expect(onsplit).toHaveBeenCalledExactlyOnceWith(true)
  await user.clear(input)
  await user.type(input, 'chia doi sang{Enter}')
  expect(onsplit).toHaveBeenLastCalledWith(false)
  await user.clear(input)
  await user.type(input, 'tap trung{Enter}')
  expect(onzen).toHaveBeenCalledOnce()
})

test('over the dashboard there is nothing to split: the palette does not offer it', () => {
  open({ onzen: vi.fn() })
  expect(options().slice(0, 2)).toEqual(['Snippets', 'Focus mode'])
})
