import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import { setLang } from './i18n.svelte'
import Shortcuts from './Shortcuts.svelte'

const open = (props: { view?: 'dashboard' | 'sessions' | 'files' } = {}) => {
  const onclose = vi.fn()
  render(Shortcuts, { onclose, ...props })
  return { onclose, user: userEvent.setup(), field: screen.getByRole('textbox') }
}
const groups = () => screen.getAllByRole('region').map((group) => group.getAttribute('aria-label'))
/** The rows of a group: what a key does, then its keys. */
const rows = (group: string) =>
  within(screen.getByRole('region', { name: group }))
    .getAllByRole('term')
    .map((term) => [term.textContent, [...term.nextElementSibling!.querySelectorAll('kbd')].map((key) => key.textContent)])

test('the cheatsheet: a dialog with the keys in groups, each key a key cap', () => {
  open()
  expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeInTheDocument()
  expect(groups()).toEqual(['Anywhere', 'Hosts', 'In a session, after Ctrl+A', 'Files', 'With the mouse', 'Command results'])
  expect(rows('Anywhere')).toEqual([
    ['Command palette', ['Ctrl+K', 'Ctrl+Shift+K']],
    ['Keyboard shortcuts', ['?', 'F1', 'Ctrl+/']],
    ['New tab: quick connect', ['Ctrl+T']],
    ['Close tab', ['Ctrl+Shift+W', 'Ctrl+W']],
    ['Next tab', ['Ctrl+Tab']],
    ['Previous tab', ['Ctrl+Shift+Tab']],
    ['Split right', ['Ctrl+Shift+D']],
    ['Split down', ['Ctrl+Shift+E']],
    ['Focus mode: hide the sidebar', ['Ctrl+Shift+B', 'Ctrl+B']],
    ['Zoom in UI', ['Ctrl+=', 'Ctrl++']],
    ['Zoom out UI', ['Ctrl+-']],
    ['Reset UI zoom', ['Ctrl+0']],
    ['Paste into the session in front', ['Ctrl+Shift+V']],
    ['close', ['Esc']],
  ])
  // After the leader the split has tmux's keys, and the two that draw the line they make.
  expect(rows('In a session, after Ctrl+A')).toContainEqual(['Split right', ['|', '%']])
  expect(rows('In a session, after Ctrl+A')).toContainEqual(['Split down', ['-', '"']])
  expect(rows('In a session, after Ctrl+A')).toContainEqual(['Focus mode: hide the sidebar', ['Z']])
  // What only the pointer does is said too.
  expect(rows('With the mouse')).toEqual([
    ['Close tab', ['Middle-click']],
    ['Rename tab', ['Double-click']],
    ['A tab onto an edge of what is open: split there, or change places', ['Drag']],
  ])
  expect(rows('Hosts')).toContainEqual(['Forwards', ['L']])
  expect(rows('Hosts')).toContainEqual(['Delete', ['D', 'Del']])
  expect(rows('In a session, after Ctrl+A')).toContainEqual(['Copy the last command\'s output', ['Y']])
  expect(rows('Files')).toContainEqual(['Parent folder', ['Backspace', '←', 'H', 'U']])
})

test('the keys of the view in front come first', () => {
  open({ view: 'sessions' })
  expect(groups()[0]).toBe('In a session, after Ctrl+A')
})

test('the field narrows the rows, by what a key does or by the key; nothing found says so', async () => {
  const { user, field } = open({ view: 'dashboard' })
  expect(field).toHaveAccessibleName('Search shortcuts')
  await user.type(field, 'tab')
  expect(groups()).toEqual(['Anywhere', 'In a session, after Ctrl+A', 'With the mouse'])
  expect(rows('Anywhere').map(([label]) => label)).toEqual(['New tab: quick connect', 'Close tab', 'Next tab', 'Previous tab'])
  await user.clear(field)
  await user.type(field, 'ctrl+f')
  expect(rows('Hosts')).toEqual([['Find a host, or type user@hostname to connect', ['/', 'Ctrl+F']]])
  expect(groups()).toEqual(['Hosts', 'Files', 'Command results'])
  await user.type(field, 'zzz')
  expect(screen.queryAllByRole('region')).toHaveLength(0)
  expect(screen.getByRole('status')).toHaveTextContent('No shortcut matches.')
})

test('closing the dialog tells the app; in Vietnamese it is Vietnamese', async () => {
  setLang('vi')
  const { onclose, user, field } = open({ view: 'files' })
  expect(screen.getByRole('dialog', { name: 'Phím tắt' })).toBeInTheDocument()
  expect(groups()).toEqual(['Tệp', 'Mọi nơi', 'Máy chủ', 'Trong phiên, sau Ctrl+A', 'Bằng chuột', 'Kết quả lệnh'])
  expect(rows('Tệp')).toContainEqual(['Đổi tên, Xoá, Tạo thư mục', ['Chuột phải']])
  await user.type(field, 'phát')
  expect(rows('Trong phiên, sau Ctrl+A')).toEqual([
    ['Bật hoặc tắt phát ở tab này', ['B']],
    ['Bật hoặc tắt phát ở mọi tab', ['Shift+B']],
  ])
  screen.getByRole<HTMLDialogElement>('dialog').close()
  expect(onclose).toHaveBeenCalledOnce()
})

test('keys that do the same are said as that: a stroke between them, the first one the key', () => {
  open()
  const next = within(screen.getByRole('region', { name: 'In a session, after Ctrl+A' })).getByText('Next tab').nextElementSibling!
  expect(next.textContent?.replace(/\s+/g, ' ').trim()).toBe('Tab / N')
  expect([...next.querySelectorAll('kbd')].map((key) => key.className.includes('opacity-60'))).toEqual([false, true])
  // One key: no stroke.
  expect(within(screen.getByRole('region', { name: 'Anywhere' })).getByText('New tab: quick connect').nextElementSibling!.textContent).not.toContain('/')
})
