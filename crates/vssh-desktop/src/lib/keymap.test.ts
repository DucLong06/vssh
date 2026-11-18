import { afterEach, expect, test } from 'vitest'
import { setLang } from './i18n.svelte'
import { capital, KEY, sheet, tip } from './keymap'

afterEach(() => setLang('en'))

/** Every key of the cheatsheet, as it is written there. */
const listed = () => new Set(sheet().flatMap((group) => group.rows.flatMap(([keys]) => keys)))

test('a tooltip is what the button does and its keys, as the cheatsheet writes them', () => {
  expect(tip('Files', KEY.files)).toBe('Files (F)')
  expect(tip(capital('close'), KEY.closeTab)).toBe('Close (Ctrl+W)')
  expect(tip('Filter', KEY.search)).toBe('Filter (/, Ctrl+F)')
  // A host's name is left as it is typed.
  expect(tip('web-01', ['Ctrl+A 1'])).toBe('web-01 (Ctrl+A 1)')
  expect(capital('')).toBe('')
})

test('no orphans: every key a button names in its tooltip has its row in the cheatsheet', () => {
  const rows = sheet().flatMap((group) => group.rows.map(([keys]) => keys))
  const orphans = Object.entries(KEY).filter(([, keys]) => !rows.includes(keys))
  expect(orphans).toEqual([])
})

test('no orphans: every key the window declares (aria-keyshortcuts) is in the cheatsheet', () => {
  const sources = import.meta.glob(['../**/*.svelte'], { query: '?raw', import: 'default', eager: true })
  const declared = Object.values(sources).flatMap((code) =>
    [...String(code).matchAll(/aria-keyshortcuts="([^"]+)"/g)].flatMap((found) => found[1].split(' ')),
  )
  // As the cheatsheet writes a key: Ctrl for Control, an arrow for its name, a letter as its key cap.
  const arrows: Record<string, string> = { ArrowLeft: '←', ArrowRight: '→', ArrowUp: '↑', ArrowDown: '↓', Delete: 'Del' }
  const written = (key: string) =>
    key
      .split('+')
      .map((part) => (part === 'Control' ? 'Ctrl' : (arrows[part] ?? (part.length === 1 ? part.toUpperCase() : part))))
      .join('+')
  // The handlers that have keys do declare them: the top bar, the dashboard and its cards, the file panes, a batch.
  expect(declared.length).toBeGreaterThan(40)
  const keys = listed()
  expect([...new Set(declared)].filter((key) => !keys.has(written(key)))).toEqual([])
})

test('the cheatsheet has a word for every row in both languages, and no key twice in a group', () => {
  for (const lang of ['en', 'vi'] as const) {
    setLang(lang)
    for (const group of sheet()) {
      expect(group.name).not.toBe('')
      const keys = group.rows.flatMap(([keys]) => keys)
      expect(new Set(keys).size, `${lang} ${group.name}`).toBe(keys.length)
      for (const [, label] of group.rows) expect(label.trim()).not.toBe('')
      // A row is found by its name: two rows of a group are not called the same.
      expect(new Set(group.rows.map(([, label]) => label)).size).toBe(group.rows.length)
    }
  }
})

test('the desktop keys are there beside the leader\'s: tabs, find, a new tab, the cheatsheet itself', () => {
  const keys = listed()
  for (const key of ['Ctrl+K', 'Ctrl+T', 'Ctrl+W', 'Ctrl+Tab', 'Ctrl+Shift+Tab', 'Ctrl+F', 'F1', 'Ctrl+/', '?', 'Ctrl+A']) {
    expect(keys.has(key), key).toBe(true)
  }
})
