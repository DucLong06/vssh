import { fireEvent, render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import { setLang } from './i18n.svelte'
import Tabs, { type Tab } from './Tabs.svelte'

const session = (id: number, title: string, over: Partial<Tab> = {}): Tab => ({ id, title, state: 'up', busy: null, broadcast: false, ...over })
const open = (props: Partial<Parameters<typeof render<typeof Tabs>>[1]> = {}) => {
  const on = {
    onpick: vi.fn(),
    onclose: vi.fn(),
    onnew: vi.fn(),
    onsplit: vi.fn(),
    onunsplit: vi.fn(),
    onzen: vi.fn(),
    onrename: vi.fn(),
    ondrag: vi.fn(),
  }
  render(Tabs, { sessions: [session(1, 'web-01'), session(2, 'web-02')], files: [{ id: -1, host: 'web-01' }], active: 1, ...on, ...props })
  return { on, user: userEvent.setup() }
}
const tab = (name: RegExp) => screen.getByRole('tab', { name })

test('a tab is a pill with its state, its number and a way to close it; a click picks it, the wheel\'s button closes it', async () => {
  const { on, user } = open()
  expect(screen.getAllByRole('tab').map((tab) => tab.textContent?.replace(/\s+/g, ' ').trim())).toEqual(['0 Hosts', '● 1 web-01', '● 2 web-02', '3 Files web-01'])
  expect(within(tab(/1 web-01/)).getByRole('img', { name: 'connected' })).toBeInTheDocument()
  await user.click(tab(/2 web-02/))
  expect(on.onpick).toHaveBeenCalledWith(2)
  await user.click(screen.getByRole('button', { name: 'close web-02' }))
  expect(on.onclose).toHaveBeenLastCalledWith(2)
  // The middle button on a tab, a session's or a file view's; no other button does it.
  await fireEvent(tab(/1 web-01/), new MouseEvent('auxclick', { button: 1, bubbles: true }))
  expect(on.onclose).toHaveBeenLastCalledWith(1)
  await fireEvent(tab(/Files web-01/), new MouseEvent('auxclick', { button: 1, bubbles: true }))
  expect(on.onclose).toHaveBeenLastCalledWith(-1)
  await fireEvent(tab(/2 web-02/), new MouseEvent('auxclick', { button: 2, bubbles: true }))
  expect(on.onclose).toHaveBeenCalledTimes(3)
  // The dashboard's tab is not one to close.
  await fireEvent(tab(/Hosts/), new MouseEvent('auxclick', { button: 1, bubbles: true }))
  expect(on.onclose).toHaveBeenCalledTimes(3)
})

test('the buttons of the strip say their keys in their tooltips: a new tab, split right, split down', async () => {
  const { on, user } = open()
  const plus = screen.getByRole('button', { name: 'New tab: quick connect' })
  expect(plus).toHaveAttribute('title', 'New tab: quick connect (Ctrl+T)')
  await user.click(plus)
  expect(on.onnew).toHaveBeenCalledOnce()
  const right = screen.getByRole('button', { name: 'Split right' })
  expect(right).toHaveAttribute('title', 'Split right (Ctrl+Shift+D)')
  await user.click(right)
  expect(on.onsplit).toHaveBeenLastCalledWith(false)
  const down = screen.getByRole('button', { name: 'Split down' })
  expect(down).toHaveAttribute('title', 'Split down (Ctrl+Shift+E)')
  await user.click(down)
  expect(on.onsplit).toHaveBeenLastCalledWith(true)
  // Nothing is split yet: nothing to undo.
  expect(screen.queryByRole('button', { name: /Unsplit/ })).toBeNull()
})

test('over the dashboard there is nothing to split', () => {
  open({ active: null })
  expect(screen.queryByRole('button', { name: 'Split right' })).toBeNull()
  expect(screen.getByRole('button', { name: 'New tab: quick connect' })).toBeInTheDocument()
})

test('the panes of a split are marked on their tabs, and the one in front can leave it', async () => {
  const { on, user } = open({ split: [1, -1] })
  expect(tab(/1 web-01/).parentElement).toHaveAttribute('data-pane')
  expect(tab(/Files web-01/).parentElement).toHaveAttribute('data-pane')
  expect(tab(/2 web-02/).parentElement).not.toHaveAttribute('data-pane')
  const whole = screen.getByRole('button', { name: 'Unsplit: this pane back to a tab of its own' })
  expect(whole).toHaveAttribute('title', 'Unsplit: this pane back to a tab of its own (Ctrl+A !)')
  await user.click(whole)
  expect(on.onunsplit).toHaveBeenCalledOnce()
})

test('a tab in front that is no pane of the split has nothing to leave', () => {
  open({ split: [2, -1] })
  expect(screen.queryByRole('button', { name: /Unsplit/ })).toBeNull()
})

test('a double click on a session\'s tab is a field for its name: Enter keeps it, Esc does not', async () => {
  const { on, user } = open()
  await user.dblClick(tab(/1 web-01/))
  const field = screen.getByRole('textbox', { name: 'Rename tab' })
  expect(field).toHaveFocus()
  // While its name is typed the pointer selects letters: the tab is not picked up.
  expect(field.parentElement).toHaveAttribute('draggable', 'false')
  expect(field).toHaveValue('web-01')
  // What was there is selected: typing replaces it.
  await user.keyboard(' deploy {Enter}')
  expect(on.onrename).toHaveBeenCalledWith(1, 'deploy')
  expect(screen.queryByRole('textbox')).toBeNull()

  await user.dblClick(tab(/2 web-02/))
  await user.keyboard('nothing{Escape}')
  expect(screen.queryByRole('textbox')).toBeNull()
  expect(on.onrename).toHaveBeenCalledOnce()
  // Left for somewhere else, the name is kept as Enter keeps it; empty is the host's name again.
  await user.dblClick(tab(/2 web-02/))
  await user.clear(screen.getByRole('textbox'))
  await user.click(tab(/Hosts/))
  expect(on.onrename).toHaveBeenLastCalledWith(2, '')
  // A file view is called after its host, and so is the dashboard's tab.
  await user.dblClick(tab(/Files web-01/))
  expect(screen.queryByRole('textbox')).toBeNull()
})

test('a tab can be picked up: the app is told which, and when it is let go', async () => {
  const { on } = open()
  // The pill is what is picked up: a web view may start no drag from the button in it.
  expect(tab(/1 web-01/).parentElement).toHaveAttribute('draggable', 'true')
  expect(tab(/1 web-01/)).not.toHaveAttribute('draggable')
  expect(tab(/Files web-01/).parentElement).toHaveAttribute('draggable', 'true')
  expect(tab(/Hosts/).parentElement).not.toHaveAttribute('draggable')
  await fireEvent.dragStart(tab(/2 web-02/))
  expect(on.ondrag).toHaveBeenLastCalledWith(2)
  await fireEvent.dragEnd(tab(/2 web-02/))
  expect(on.ondrag).toHaveBeenLastCalledWith(null)
})

test('in focus mode the strip has the way back to the sidebar, and says a leader that waits; in Vietnamese too', async () => {
  setLang('vi')
  const { on, user } = open({ zen: true, leader: true })
  const back = screen.getByRole('button', { name: 'Hiện thanh bên' })
  expect(back).toHaveAttribute('title', 'Hiện thanh bên (Ctrl+Shift+B, Ctrl+B)')
  await user.click(back)
  expect(on.onzen).toHaveBeenCalledOnce()
  expect(screen.getByRole('status')).toHaveTextContent('Ctrl+A')
  expect(screen.getByRole('button', { name: 'Chia đôi sang phải' })).toHaveAttribute('title', 'Chia đôi sang phải (Ctrl+Shift+D)')
  expect(screen.getByRole('button', { name: 'Chia đôi xuống dưới' })).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Tab mới: kết nối nhanh' })).toBeInTheDocument()
})

test('with the sidebar there, the strip has no button for it and no leader to say', () => {
  open()
  expect(screen.queryByRole('button', { name: 'Show the sidebar' })).toBeNull()
  expect(screen.queryByRole('status')).toBeNull()
})

// --- D12: broadcast, from the strip ---------------------------------------------

const cast = () => screen.queryByRole('button', { name: 'broadcast' })

test('the broadcast button is there once two sessions are open and not before: one session has nobody to broadcast to, and a host\'s files are no session', async () => {
  const onbroadcast = vi.fn()
  const { rerender } = render(Tabs, { sessions: [], active: null, onpick: vi.fn(), onclose: vi.fn(), onbroadcast })
  expect(cast()).toBeNull()
  await rerender({ sessions: [session(1, 'web-01')], files: [{ id: -1, host: 'web-01' }, { id: -2, host: 'web-02' }], active: 1 })
  expect(cast()).toBeNull()
  await rerender({ sessions: [session(1, 'web-01'), session(2, 'web-01')] })
  expect(cast()).toBeInTheDocument()
  // On the dashboard too: what is open is open behind it.
  await rerender({ active: null })
  expect(cast()).toBeInTheDocument()
  // One of the two closed: the button goes with it.
  await rerender({ sessions: [session(2, 'web-01')] })
  expect(cast()).toBeNull()
})

test('nobody to tell: without a way to ask for it the strip has no broadcast button', () => {
  open()
  expect(cast()).toBeNull()
})

test('the broadcast button is a switch: off it asks for all of them, and says in its tooltip what it does and its key; on it is lit, and asks for none', async () => {
  const onbroadcast = vi.fn()
  const user = userEvent.setup()
  const sessions = [session(1, 'web-01'), session(2, 'web-02'), session(3, 'db-primary')]
  const { rerender } = render(Tabs, { sessions, active: 1, onpick: vi.fn(), onclose: vi.fn(), onbroadcast })
  const button = cast()!
  expect(button).toHaveAttribute('aria-pressed', 'false')
  expect(button).toHaveAccessibleName('broadcast')
  expect(button).toHaveAttribute('title', 'Broadcast: type once, run in every open session (Ctrl+A Shift+B)')
  // Lit and with a line around it only while it is on.
  expect(button).toHaveClass('aria-pressed:border-yellow', 'aria-pressed:text-yellow', 'border-transparent')
  await user.click(button)
  expect(onbroadcast).toHaveBeenLastCalledWith(true)

  await rerender({ sessions: sessions.map((tab) => ({ ...tab, broadcast: true })) })
  expect(button).toHaveAttribute('aria-pressed', 'true')
  // No wider for being on: the tabs beside it keep their room.
  expect(button).toHaveAccessibleName('broadcast')
  // One pane taken out of it by its own switch: still on, for the two that are left.
  await rerender({ sessions: sessions.map((tab) => ({ ...tab, broadcast: tab.id !== 2 })) })
  expect(button).toHaveAttribute('aria-pressed', 'true')
  await user.click(button)
  expect(onbroadcast).toHaveBeenLastCalledWith(false)
})

test('the broadcast button in Vietnamese', () => {
  setLang('vi')
  render(Tabs, { sessions: [session(1, 'web-01'), session(2, 'web-02')], active: 1, onpick: vi.fn(), onclose: vi.fn(), onbroadcast: vi.fn() })
  const button = screen.getByRole('button', { name: 'phát' })
  expect(button).toHaveAttribute('title', 'Phát: gõ một lần, chạy ở mọi phiên đang mở (Ctrl+A Shift+B)')
  setLang('en')
})
