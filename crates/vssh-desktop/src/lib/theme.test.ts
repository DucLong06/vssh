import { expect, test, vi } from 'vitest'
import css from '../app.css?raw'
import { THEME } from './term'
import { setTheme, theme } from './theme.svelte'

/** The colour tokens of one block of app.css, by name. */
function tokens(block: string): Record<string, string> {
  return Object.fromEntries([...block.matchAll(/--color-([a-z]+):\s*(#[0-9a-f]{6})/g)].map(([, name, hex]) => [name, hex]))
}
const night = tokens(css.slice(css.indexOf('@theme'), css.indexOf("[data-theme='day']")))
const day = tokens(css.slice(css.indexOf("[data-theme='day']")))

// WCAG 2.1: relative luminance, and the contrast of two colours.
const rgb = (hex: string) => [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16))
const linear = (part: number) => (part <= 0.04045 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4)
function luminance(hex: string) {
  const [r, g, b] = rgb(hex).map((part) => linear(part / 255))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}
function contrast(a: string, b: string) {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (light + 0.05) / (dark + 0.05)
}
/** `share` of one colour over another, as `color-mix(in srgb, …)` over an opaque ground comes out. */
const over = (colour: string, ground: string, share: number) =>
  `#${rgb(colour).map((part, at) => Math.round(part * share + rgb(ground)[at] * (1 - share)).toString(16).padStart(2, '0')).join('')}`

/** What text is drawn on, and what it is drawn in. `edge` is borders only. */
const GROUNDS = ['bg', 'panel', 'black', 'bar', 'selection']
const INKS = ['fg', 'white', 'dim', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'orange', 'teal']
/** The colours of the tag pills (`chip` in hosts.ts). */
const CHIPS = ['blue', 'magenta', 'cyan', 'orange', 'teal', 'yellow']

test('the check itself: black on white is 21:1, and the Night theme\'s text on its background is the 8.1:1 app.css says', () => {
  expect(contrast('#000000', '#ffffff')).toBeCloseTo(21, 5)
  expect(contrast(night.white, night.bg)).toBeCloseTo(8.1, 1)
  expect(over('#000000', '#ffffff', 0.5)).toBe('#808080')
})

test('Day has a colour for everything Night has one for, but the terminal', () => {
  expect(Object.keys(day).sort()).toEqual(Object.keys(night).filter((name) => name !== 'term').sort())
})

test('Day is TokyoNight Day: its grounds and its border are the palette\'s own', () => {
  // extras/lua/tokyonight_day.lua: bg, bg_dark (the sidebar's, the floats'), bg_dark1, bg_highlight, terminal_black.
  expect(day).toMatchObject({ bg: '#e1e2e7', black: '#d0d5e3', panel: '#d0d5e3', bar: '#c1c9df', selection: '#c4c8da', edge: '#a1a6c5' })
})

test('in Day every text colour is at least 4.5:1 on everything text is drawn on', () => {
  const low = INKS.flatMap((ink) =>
    GROUNDS.filter((ground) => contrast(day[ink], day[ground]) < 4.5).map(
      (ground) => `${ink} on ${ground}: ${contrast(day[ink], day[ground]).toFixed(2)}`,
    ),
  )
  expect(low).toEqual([])
})

test('in Day a tag pill, which is tinted with its own colour, is at least 4.5:1 too', () => {
  // HostCard: the tag's colour, 12% of it over the card.
  const low = CHIPS.map((chip) => [chip, contrast(day[chip], over(day[chip], day.panel, 0.12))] as const)
    .filter(([, ratio]) => ratio < 4.5)
    .map(([chip, ratio]) => `${chip}: ${ratio.toFixed(2)}`)
  expect(low).toEqual([])
})

test('the terminal is dark in both themes: its ground is Night\'s, and Day does not touch it', () => {
  expect(night.term).toBe(THEME.background)
  expect(night.term).toBe(night.bg)
  expect(day.term).toBeUndefined()
})

test('Night is the theme until Day is chosen; the choice is kept as theme and read when the app starts', async () => {
  expect(theme.name).toBe('night')
  expect(document.documentElement.dataset.theme).toBe('night')
  setTheme('day')
  expect(document.documentElement.dataset.theme).toBe('day')
  expect(localStorage.getItem('vssh.theme')).toBe('day')
  vi.resetModules()
  expect((await import('./theme.svelte')).theme.name).toBe('day')
  // What is neither is Night.
  localStorage.setItem('vssh.theme', 'solarized')
  vi.resetModules()
  expect((await import('./theme.svelte')).theme.name).toBe('night')
  expect(document.documentElement.dataset.theme).toBe('night')
})

test('flat: a shadow is for what floats over the page (a dialog, a menu, a popover), and nothing else has one', () => {
  const sources = import.meta.glob(['../**/*.svelte'], { query: '?raw', import: 'default', eager: true })
  const shadowed = Object.entries(sources)
    .filter(([, code]) => /\bshadow-|box-shadow|drop-shadow/.test(String(code)))
    .map(([file, code]) => [file.split('/').pop(), [...new Set(String(code).match(/\bshadow-[\w/[\]-]+/g))]])
  expect(shadowed).toEqual([
    ['Menu.svelte', ['shadow-float']],
    ['Modal.svelte', ['shadow-float']],
    ['QuickPeek.svelte', ['shadow-float']],
  ])
  // The token the cards had is there and is nothing, in both themes; no rule draws a shadow of its own.
  expect(css.match(/--shadow-card: none;/g)).toHaveLength(2)
  expect(css.match(/--shadow-float:/g)).toHaveLength(2)
  expect(css).not.toMatch(/box-shadow/)
  // Nor a glow in a colour, nor a wash down a card.
  const all = Object.values(sources).join('\n')
  expect(all).not.toMatch(/shadow-(blue|red|black)|bg-linear/)
})
