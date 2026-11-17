import { render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import { i18n, setLang } from './i18n.svelte'
import { setStreamer, streamer } from './streamer.svelte'
import Topbar from './Topbar.svelte'

afterEach(() => {
  setLang('en')
  setStreamer(false)
})

test('the head of a view: its name as the heading, a line about it, and the way to the palette', async () => {
  const user = userEvent.setup()
  const onpalette = vi.fn()
  render(Topbar, { title: 'Status', sub: '4 Hosts · 3 reachable · 1 down', onpalette })
  expect(screen.getByRole('heading', { level: 1, name: 'Status' })).toHaveClass('text-2xl', 'font-semibold')
  expect(screen.getByRole('banner')).toHaveTextContent('4 Hosts · 3 reachable · 1 down')
  const search = screen.getByRole('button', { name: 'Command palette' })
  expect(search).toHaveTextContent('Search hosts and actions Ctrl-K')
  await user.click(search)
  expect(onpalette).toHaveBeenCalledOnce()
})

test('the language is one button drawn as the pair it chooses between; the one in use is lit', async () => {
  const user = userEvent.setup()
  render(Topbar, { title: 'Status', onpalette: vi.fn() })
  const toggle = screen.getByRole('button', { name: 'Language: EN' })
  expect(toggle).toHaveTextContent('ENVI')
  const lit = () => [...toggle.querySelectorAll('span')].filter((half) => half.classList.contains('bg-selection')).map((half) => half.textContent)
  expect(lit()).toEqual(['EN'])
  await user.click(toggle)
  expect(i18n.lang).toBe('vi')
  expect(toggle).toHaveAccessibleName('Ngôn ngữ: VI')
  expect(lit()).toEqual(['VI'])
})

test('streamer mode is switched here, and says whether it is on', async () => {
  const user = userEvent.setup()
  render(Topbar, { title: 'Status', onpalette: vi.fn() })
  const toggle = screen.getByRole('button', { name: /Streamer mode/ })
  expect(toggle).toHaveAttribute('aria-pressed', 'false')
  await user.click(toggle)
  expect(streamer.on).toBe(true)
  expect(toggle).toHaveAttribute('aria-pressed', 'true')
})

test('a host or an address for a title is in the monospace face', () => {
  render(Topbar, { title: 'deploy@10.0.0.9', mono: true, onpalette: vi.fn() })
  expect(screen.getByRole('heading', { name: 'deploy@10.0.0.9' })).toHaveClass('font-mono')
})
