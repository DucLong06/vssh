// The window's keys, once: a button's tooltip and the cheatsheet read them
// here, so neither can name a key the other does not. The handlers are where
// they were (App, Dashboard, FilePane, Batch); keymap.test.ts holds the
// `aria-keyshortcuts` they declare against this table.
import { t } from './i18n.svelte'

/** The keys a button names in its tooltip. Several: any of them. */
export const KEY = {
  // In a terminal the plain chord is the shell's own: there, and anywhere, the one with Shift.
  palette: ['Ctrl+K', 'Ctrl+Shift+K'],
  shortcuts: ['?', 'F1', 'Ctrl+/'],
  newTab: ['Ctrl+T'],
  closeTab: ['Ctrl+Shift+W', 'Ctrl+W'],
  // With Shift: no shell is sent these, so they are the window's in a terminal too.
  splitRight: ['Ctrl+Shift+D'],
  splitDown: ['Ctrl+Shift+E'],
  focus: ['Ctrl+Shift+B', 'Ctrl+B'],
  leader: ['Ctrl+A'],
  search: ['/', 'Ctrl+F'],
  add: ['A'],
  snippets: ['S'],
  run: ['X'],
  connect: ['Enter'],
  peek: ['V'],
  refresh: ['R'],
  files: ['F'],
  forwards: ['L'],
  edit: ['E'],
  copy: ['C'],
  hidden: ['.'],
  copyAll: ['Y'],
  // With Shift: plain Ctrl-V is the shell's own (the next key, as it is).
  paste: ['Ctrl+Shift+V'],
} as const

type Keys = readonly string[]
/** The view a group's keys work in: the rail's name for it. */
export type View = 'dashboard' | 'sessions' | 'files'
export type Group = { name: string; view?: View; rows: [keys: Keys, label: string][] }

/** Focus mode, and what it does: the rail's own button says the second half. */
const focus = () => `${t('focus_mode')}: ${t('hide_sidebar').toLowerCase()}`

/** Every key there is, by where it works, in the window's language. */
export const sheet = (): Group[] => [
  {
    name: t('keys_anywhere'),
    rows: [
      [KEY.palette, t('palette')],
      [KEY.shortcuts, t('shortcuts')],
      [KEY.newTab, t('new_tab')],
      [KEY.closeTab, t('close_tab')],
      [['Ctrl+Tab'], t('next_tab')],
      [['Ctrl+Shift+Tab'], t('prev_tab')],
      [KEY.splitRight, t('split_right')],
      [KEY.splitDown, t('split_down')],
      [KEY.focus, focus()],
      [KEY.paste, t('paste_session')],
      [['Esc'], t('close')],
    ],
  },
  {
    name: t('hosts'),
    view: 'dashboard',
    rows: [
      [KEY.search, t('search_hosts')],
      [KEY.add, t('add_host')],
      [KEY.snippets, t('snippets')],
      [KEY.run, t('run_command')],
      [KEY.connect, t('connect')],
      [['Space'], t('select')],
      [KEY.peek, t('quick_peek')],
      [KEY.refresh, `${t('quick_peek')}: ${t('refresh')}`],
      [KEY.files, t('files')],
      [KEY.forwards, t('forwards')],
      [KEY.edit, t('edit')],
      // Del, as the key cap has it: `Delete` is also the English for what it does.
      [['D', 'Del'], t('delete')],
      [['Alt+←', 'Alt+→'], t('reorder')],
    ],
  },
  {
    name: t('keys_session'),
    view: 'sessions',
    rows: [
      [['0–9'], t('go_to_tab')],
      [['Tab', 'N'], t('next_tab')],
      [['Shift+Tab', 'P'], t('prev_tab')],
      [['Q'], t('back_to_hosts')],
      [['Y'], t('copy_output')],
      [['B'], t('broadcast_tab')],
      [['Shift+B'], t('broadcast_all')],
      [['X', 'D'], t('close_tab')],
      // tmux's own, and the two that draw the line they make.
      [['|', '%'], t('split_right')],
      [['-', '"'], t('split_down')],
      [['!'], t('unsplit')],
      [['{', '}'], t('swap_panes')],
      [['Z'], focus()],
      [KEY.leader, t('send_to_shell')],
      [['?'], t('shortcuts')],
    ],
  },
  {
    name: t('files'),
    view: 'files',
    rows: [
      [['↑', '↓', 'J', 'K'], t('move')],
      [['Home', 'End', 'G', 'Shift+G'], t('first_last')],
      [['Enter', '→', 'L'], t('open_folder')],
      [['Backspace', '←', 'H', 'U'], t('parent_folder')],
      [['Space'], t('select')],
      [KEY.copy, t('copy')],
      [KEY.search, t('filter')],
      [KEY.hidden, t('hidden_files')],
      [[t('right_click')], `${t('rename')}, ${t('delete')}, ${t('new_folder')}`],
    ],
  },
  {
    name: t('keys_mouse'),
    rows: [
      [[t('middle_click')], t('close_tab')],
      [[t('double_click')], t('rename_tab')],
      [[t('drag')], t('drag_tab')],
    ],
  },
  {
    name: t('keys_results'),
    rows: [
      [['J', 'K'], t('scroll')],
      [KEY.search, t('filter')],
      [KEY.copyAll, t('copy_all')],
    ],
  },
]

/** A button's tooltip: what it does, and its key. */
export const tip = (label: string, keys: Keys) => `${label} (${keys.join(', ')})`

/** A word the table has in small letters, where it starts a tooltip. */
export const capital = (word: string) => word.charAt(0).toUpperCase() + word.slice(1)
