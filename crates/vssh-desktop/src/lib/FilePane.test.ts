import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import listing from '../../src-tauri/tests/fixtures/files.json'
import type { Entry, Shown } from './api'
import errors from './fixtures/errors.json'
import shown from './fixtures/shown.json'
import FilePane from './FilePane.svelte'
import { when } from './paths'
import { setLang } from './i18n.svelte'

const timeout = errors.findIndex((error) => error.class === 'timeout')
const dotted: Entry[] = [...listing, { name: '.profile', is_dir: false, size: 807, mtime: 0 }]
const many: Entry[] = ['a', 'b', 'c', 'd'].map((name) => ({ name, is_dir: name < 'c', size: 1, mtime: 0 }))

function pane(props: Partial<Parameters<typeof render<typeof FilePane>>[1] & object> = {}) {
  const heard = { oncd: vi.fn(), oncopy: vi.fn(), onmenu: vi.fn(), onretry: vi.fn(), ondragnames: vi.fn(), ondropped: vi.fn() }
  const view = render(FilePane, { side: 'remote', label: 'web-01', path: '/var/log', entries: listing, ...heard, ...props })
  return { ...heard, view, user: userEvent.setup(), grid: screen.getByRole('grid', { name: 'web-01' }) }
}
const rows = () => screen.getAllByRole('row').slice(1)
const names = () => rows().map((row) => within(row).getAllByRole('gridcell')[0].textContent?.trim())
afterEach(() => vi.restoreAllMocks())
const current = () => rows().find((row) => row.getAttribute('aria-selected') === 'true')

test('a directory: a row per entry with its name, size and when it changed; directories are told apart', () => {
  pane()
  expect(screen.getAllByRole('columnheader').map((th) => th.textContent)).toEqual(['Name', 'Size', 'Modified'])
  expect(names()).toEqual(['nginx', 'auth.log', 'empty'])
  const [dir, file, unknown] = rows()
  expect(dir).toHaveAttribute('data-kind', 'dir')
  expect(file).toHaveAttribute('data-kind', 'file')
  // An icon each, and not the same one.
  expect(dir.querySelector('svg')).not.toBeNull()
  expect(dir.querySelector('svg')?.innerHTML).not.toBe(file.querySelector('svg')?.innerHTML)
  // A directory's size says nothing about what is in it.
  expect(within(dir).getAllByRole('gridcell')[1]).toHaveTextContent('—')
  expect(within(file).getAllByRole('gridcell')[1]).toHaveTextContent('17.8 K')
  expect(within(file).getAllByRole('gridcell')[2]).toHaveTextContent(when(1763371234))
  expect(within(unknown).getAllByRole('gridcell')[1]).toHaveTextContent('0 B')
  expect(within(unknown).getAllByRole('gridcell')[2]).toHaveTextContent('—')
  // The first entry is the selected one.
  expect(current()).toBe(dir)
})

test('the path is a row of crumbs: a click on one goes there', async () => {
  const { user, oncd } = pane()
  const path = screen.getByRole('navigation', { name: 'Path' })
  expect(within(path).getAllByRole('button').map((crumb) => crumb.textContent?.trim())).toEqual(['/', 'var', 'log'])
  await user.click(within(path).getByRole('button', { name: 'var' }))
  expect(oncd).toHaveBeenCalledWith('/var')
})

test('a path too long for its pane starts at the first name that fits whole, behind an ellipsis', () => {
  // jsdom lays nothing out: a trail of 450 in a pane of 200, its crumbs 100 apart.
  vi.spyOn(Element.prototype, 'scrollWidth', 'get').mockReturnValue(450)
  vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(200)
  vi.spyOn(HTMLElement.prototype, 'offsetLeft', 'get').mockImplementation(function (this: HTMLElement) {
    return [...this.parentElement!.querySelectorAll('button')].indexOf(this as HTMLButtonElement) * 100
  })
  pane({ path: '/var/log/nginx/sites' })
  const path = screen.getByRole('navigation', { name: 'Path' })
  // 250 are hidden: `log` at 200 would be cut, `nginx` at 300 is whole.
  expect(path.scrollLeft).toBe(300)
  expect(screen.getByText('…')).toBeInTheDocument()
})

test('a path that fits is shown whole, with no ellipsis', () => {
  pane()
  expect(screen.getByRole('navigation', { name: 'Path' }).scrollLeft).toBe(0)
  expect(screen.queryByText('…')).toBeNull()
})

test('an empty directory says so', () => {
  pane({ entries: [] })
  expect(rows()).toHaveLength(0)
  expect(screen.getByText('Empty directory.')).toBeInTheDocument()
})

test('a host that died: the pane says which kind of failure, and offers to try again', async () => {
  const { user, onretry } = pane({ entries: [], error: shown.en[timeout] as Shown })
  const alert = screen.getByRole('alert')
  expect(alert).toHaveTextContent('✗ TIMEOUT')
  expect(alert).toHaveTextContent(shown.en[timeout].message)
  expect(screen.queryByText('Empty directory.')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'try again' }))
  expect(onretry).toHaveBeenCalled()
})

test('while the host is asked the grid is busy, and what was there stays', () => {
  const { grid } = pane({ loading: true })
  expect(grid).toHaveAttribute('aria-busy', 'true')
  expect(rows()).toHaveLength(3)
})

test('dotfiles are hidden until . shows them', async () => {
  const { user, grid } = pane({ entries: dotted })
  expect(names()).not.toContain('.profile')
  const toggle = screen.getByRole('button', { name: 'Hidden files' })
  expect(toggle).toHaveAttribute('aria-pressed', 'false')
  grid.focus()
  await user.keyboard('.')
  expect(names()).toContain('.profile')
  expect(toggle).toHaveAttribute('aria-pressed', 'true')
  await user.click(toggle)
  expect(names()).not.toContain('.profile')
})

test('the pane in Vietnamese', () => {
  setLang('vi')
  pane({ entries: [] })
  expect(screen.getAllByRole('columnheader').map((th) => th.textContent)).toEqual(['Tên', 'Kích thước', 'Sửa đổi'])
  expect(screen.getByText('Thư mục trống.')).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Tệp ẩn' })).toBeInTheDocument()
})

// --- M2: the keys ---

test('j and k, the arrows, g and G move the selection; a click selects', async () => {
  const { user, grid } = pane({ entries: many })
  grid.focus()
  await user.keyboard('j')
  expect(current()).toHaveTextContent('b')
  await user.keyboard('{ArrowDown}')
  expect(current()).toHaveTextContent('c')
  await user.keyboard('k')
  expect(current()).toHaveTextContent('b')
  await user.keyboard('G')
  expect(current()).toHaveTextContent('d')
  // Not past the end.
  await user.keyboard('j')
  expect(current()).toHaveTextContent('d')
  await user.keyboard('g')
  expect(current()).toHaveTextContent('a')
  await user.click(rows()[2])
  expect(current()).toHaveTextContent('c')
})

test('Enter and l go into the selected directory, not into a file; Backspace and u go up', async () => {
  const { user, grid, oncd } = pane({ entries: many, path: '/srv/www' })
  grid.focus()
  await user.keyboard('{Enter}')
  expect(oncd).toHaveBeenLastCalledWith('/srv/www/a')
  await user.keyboard('jl')
  expect(oncd).toHaveBeenLastCalledWith('/srv/www/b')
  // Going in starts at the top again (here the rows stay: nobody answered). c is a file.
  await user.keyboard('jj{Enter}')
  expect(oncd).toHaveBeenCalledTimes(2)
  await user.keyboard('{Backspace}')
  expect(oncd).toHaveBeenLastCalledWith('/srv')
  await user.keyboard('u')
  expect(oncd).toHaveBeenCalledTimes(4)
  await user.dblClick(rows()[0])
  expect(oncd).toHaveBeenLastCalledWith('/srv/www/a')
})

test('back up from a directory, the directory it came out of is the selected one', async () => {
  const { user, grid, view } = pane({ entries: [], path: '/srv/www/c' })
  grid.focus()
  await user.keyboard('u')
  await view.rerender({ path: '/srv/www', entries: many })
  expect(current()).toHaveTextContent('c')
})

test('/ narrows the rows to the names that have what is typed; Escape gives them back', async () => {
  const { user, grid } = pane()
  grid.focus()
  await user.keyboard('/')
  const filter = screen.getByRole('searchbox', { name: 'Filter' })
  expect(filter).toHaveFocus()
  await user.keyboard('LOG')
  expect(names()).toEqual(['auth.log'])
  // Enter goes back to the rows and keeps them narrowed.
  await user.keyboard('{Enter}')
  expect(grid).toHaveFocus()
  expect(names()).toEqual(['auth.log'])
  await user.keyboard('/{Escape}')
  expect(names()).toEqual(['nginx', 'auth.log', 'empty'])
  expect(grid).toHaveFocus()
})

test('Space marks the selected entry and moves on; c asks for a copy of the selected one', async () => {
  const { user, grid, oncopy, view } = pane({ entries: many, marked: [] })
  grid.focus()
  await user.keyboard('j ')
  // Space was pressed on b.
  expect(rows()[1]).toHaveAttribute('data-marked')
  expect(within(rows()[1]).getByRole('img', { name: 'selected' })).toBeInTheDocument()
  expect(current()).toHaveTextContent('c')
  await user.keyboard('c')
  expect(oncopy).toHaveBeenLastCalledWith(many[2])
  // Space again takes the mark off.
  await user.keyboard('k ')
  expect(rows()[1]).not.toHaveAttribute('data-marked')
  // Ctrl-click marks too.
  await user.keyboard('{Control>}')
  await user.click(rows()[3])
  await user.keyboard('{/Control}')
  expect(rows()[3]).toHaveAttribute('data-marked')
  view.unmount()
})

test('the menu key and the right button ask for the menu of the entry under them', async () => {
  const { user, grid, onmenu } = pane({ entries: many })
  await user.pointer({ keys: '[MouseRight]', target: rows()[2] })
  expect(onmenu).toHaveBeenLastCalledWith(expect.anything(), many[2])
  expect(current()).toHaveTextContent('c')
  // On the grid itself, as the menu key does it: the selected entry's.
  grid.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  expect(onmenu).toHaveBeenLastCalledWith(expect.anything(), many[2])
})
