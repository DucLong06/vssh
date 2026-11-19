import { render, screen, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import type { ConnError, Host, Peek, Shown } from './api'
import errors from './fixtures/errors.json'
import hosts from './fixtures/hosts.json'
import fixture from './fixtures/peek.json'
import shown from './fixtures/shown.json'
import HostCard from './HostCard.svelte'
import { setLang } from './i18n.svelte'
import type { Live } from './live.svelte'

const peek = fixture as unknown as Peek
const [web01, , , bare] = hosts as Host[]
const card = (state?: Live, host = web01) => {
  render(HostCard, { host, info: state })
  return screen.getByRole('article', { name: host.alias })
}
const stats = (cpu: number | null, mem: [number, number] | null, disk: number | null): Live => ({
  ms: 12,
  peek: { ...peek, stats: { cpu, mem, disk } },
})
const meter = (name: string) => screen.getByRole('meter', { name })
const dot = (el: HTMLElement) => el.querySelector('header [aria-hidden]')

test('a healthy host is a low card of three short lines: its name and latency, user@host and its tags, and three bars with their figures', () => {
  const el = card({ ms: 12, peek })
  expect(el).toHaveAttribute('data-low')
  expect(within(el).getByRole('heading', { name: 'web-01' })).toBeInTheDocument()
  // Where it is, under its name, in the letters of what is typed; whole to the pointer where the card is too narrow for it.
  const address = within(el).getByText('deploy@10.0.0.5:2222')
  expect(address).toHaveClass('font-mono', 'truncate')
  expect(address).toHaveAttribute('title', 'deploy@10.0.0.5:2222')
  expect(address.parentElement).toContainElement(within(el).getByRole('list', { name: 'Tags' }))
  // The minute behind the figures is the quick peek's to draw.
  expect(el.querySelector('svg')).toBeNull()
  const tags = within(within(el).getByRole('list', { name: 'Tags' })).getAllByRole('listitem')
  expect(tags.map((tag) => tag.textContent)).toEqual(['web', 'eu', 'prod'])
  expect(within(el).getByText('12 ms')).toBeInTheDocument()
  expect(within(el).getByRole('img', { name: 'pinned' })).toBeInTheDocument()

  // The compact label of the brief is the bars' name.
  expect(within(el).getByRole('group', { name: 'CPU 23% · MEM 4.2/16G · / 61%' })).toBeInTheDocument()
  expect(meter('CPU')).toHaveAttribute('aria-valuenow', '23')
  expect(meter('MEM')).toHaveAttribute('aria-valuenow', '27')
  expect(meter('/')).toHaveAttribute('aria-valuenow', '61')
  for (const name of ['CPU', 'MEM', '/']) expect(meter(name)).toHaveAttribute('data-level', 'ok')
  expect(el).toHaveAttribute('data-state', 'ok')
})

test('CPU at 0%: an empty bar, still a healthy card', () => {
  const el = card(stats(0, [0, 16_000_000], 0))
  expect(meter('CPU')).toHaveAttribute('aria-valuenow', '0')
  expect(meter('CPU').firstElementChild).toHaveStyle({ width: '0%' })
  expect(el).toHaveTextContent('CPU 0%')
  expect(el).toHaveAttribute('data-state', 'ok')
})

test('CPU at 100%: a full red bar and a sick card; past 100 the figure and the bar stay at 100', () => {
  const el = card(stats(100, [15_000_000, 16_000_000], 95))
  expect(meter('CPU')).toHaveAttribute('aria-valuenow', '100')
  expect(meter('CPU')).toHaveAttribute('data-level', 'crit')
  expect(meter('CPU').firstElementChild).toHaveStyle({ width: '100%' })
  expect(meter('MEM')).toHaveAttribute('data-level', 'crit')
  expect(el).toHaveAttribute('data-state', 'crit')
  // One that needs looking at keeps its height, and its address on it.
  expect(el).not.toHaveAttribute('data-low')
  expect(el).toHaveTextContent('deploy@10.0.0.5:2222')
  // Overloaded is not down: it answers, so its dot is as green as a healthy host's.
  expect(dot(el)).toHaveClass('text-green')

  render(HostCard, { host: bare, info: stats(250, null, null) })
  const queue = screen.getByRole('article', { name: 'bare' })
  expect(queue).toHaveTextContent('CPU 100%')
  expect(queue).toHaveAttribute('data-state', 'crit')
  expect(within(queue).getByRole('meter', { name: 'CPU' })).toHaveAttribute('aria-valuenow', '100')
})

test('levels in between are warnings, not sickness', () => {
  const el = card(stats(51, [71, 100], 81))
  for (const name of ['CPU', 'MEM', '/']) expect(meter(name)).toHaveAttribute('data-level', 'warn')
  expect(el).toHaveAttribute('data-state', 'ok')
})

test('a host without meminfo: the other bars, and a dash where memory would be', () => {
  const el = card(stats(75, null, 61))
  expect(within(el).getByRole('group', { name: 'CPU 75% · / 61%' })).toBeInTheDocument()
  expect(screen.queryByRole('meter', { name: 'MEM' })).not.toBeInTheDocument()
  expect(el).toHaveTextContent('MEM —')
  expect(el.textContent).not.toMatch(/NaN|null|undefined/)
})

test('a host that answered nothing usable shows three dashes and no bar', () => {
  const el = card(stats(null, null, null))
  expect(screen.queryByRole('meter')).not.toBeInTheDocument()
  expect(el.textContent?.replace(/\s+/g, '')).toContain('CPU—MEM—/—')
})

test.each(errors.map((error, i) => [error.class, error, shown.en[i]] as const))(
  'offline, %s: the card says which kind of failure, with the whole chain behind it, and nothing on it is dimmed',
  (_, error, words) => {
    const el = card({ ms: null, error: error as ConnError, shown: words as Shown })
    expect(el).toHaveAttribute('data-state', 'down')
    expect(dot(el)).toHaveClass('text-red')
    const why = within(el).getByText(words.message)
    expect(why.parentElement).toHaveTextContent(`✗ ${words.label} ${words.message}`)
    expect(why.parentElement).toHaveAttribute('title', words.chain)
    expect(screen.queryByRole('meter')).not.toBeInTheDocument()
    // Still the same host. Its address and tags are what one reads when it is
    // down: a blue tag is 5.0:1 as it is and under 4.5:1 at any opacity below 90%.
    expect(within(el).getByRole('heading', { name: 'web-01' })).toBeInTheDocument()
    expect(within(el).getByText('deploy@10.0.0.5:2222').className).not.toContain('opacity')
    expect(within(el).getByRole('list', { name: 'Tags' }).className).not.toContain('opacity')
  },
)

test('every class of failure has its own label', () => {
  expect(shown.en.map((words) => words.label)).toEqual([
    'DNS',
    'TIMEOUT',
    'REFUSED',
    'UNREACHABLE',
    'AUTH',
    'HOST KEY',
    'JUMP HOST',
    'ERROR',
    'PORT IN USE',
  ])
})

test('a host the dashboard did not ask: "not checked", and its latency if it has one', () => {
  const el = card({ ms: 0, peek: null })
  expect(el).toHaveTextContent('not checked')
  expect(el).toHaveTextContent('<1 ms')
  expect(el).toHaveAttribute('data-state', 'unchecked')
})

test('down by ping alone, with nobody asked why', () => {
  const el = card({ ms: null, peek: null })
  expect(el).toHaveAttribute('data-state', 'down')
  expect(within(el).getByText('down')).toBeInTheDocument()
})

test('before the first answer: asking, and no latency yet', () => {
  const el = card(undefined, bare)
  expect(el).toHaveTextContent('asking bare…')
  expect(el).toHaveTextContent('—')
  expect(el).toHaveAttribute('data-state', 'asking')
  // A host with nothing but a name shows the name and nothing made up.
  expect(el.textContent).not.toMatch(/null|undefined|@|:/)
  expect(screen.queryByRole('list', { name: 'Tags' })).not.toBeInTheDocument()
})

test("in Vietnamese the states are the TUI's words", () => {
  setLang('vi')
  expect(card({ ms: null, peek: null })).toHaveTextContent('mất kết nối')
  expect(screen.getByRole('article')).toHaveTextContent('chưa kiểm tra')
  render(HostCard, { host: bare, info: { ms: 3, peek } })
  expect(screen.getByRole('group', { name: 'CPU 23% · RAM 4.2/16G · / 61%' })).toBeInTheDocument()
  render(HostCard, { host: (hosts as Host[])[1] })
  expect(screen.getByRole('article', { name: 'web-02' })).toHaveTextContent('đang hỏi web-02…')
})

test('a tall card that can open the host\'s files has a button for it, next to Connect; a low one has Connect alone, and nothing lying on its name', async () => {
  const [onfiles, onconnect] = [vi.fn(), vi.fn()]
  const low = render(HostCard, { host: web01, info: { ms: 12, peek }, onfiles, onconnect })
  expect(screen.getAllByRole('button').map((button) => button.textContent?.trim())).toEqual(['Connect'])
  low.unmount()
  render(HostCard, { host: web01, info: stats(100, null, null), onfiles, onconnect })
  const buttons = screen.getAllByRole('button').map((button) => button.textContent?.trim())
  expect(buttons).toEqual(['Files', 'Connect'])
  await userEvent.click(screen.getByRole('button', { name: 'Files' }))
  expect(onfiles).toHaveBeenCalledOnce()
  expect(onconnect).not.toHaveBeenCalled()
})

// --- D9: the concept's card ----------------------------------------------------

test('on a host past a mark CPU and memory have a sparkline each, drawn from the minute behind the figure; the disk has its bar alone', () => {
  const { container } = render(HostCard, { host: web01, info: stats(23, [4_200_000, 16_000_000], 95), trail: { cpu: [0, 50, 100], mem: [25, 75] } })
  const lines = [...container.querySelectorAll('svg path[stroke]')].map((path) => path.getAttribute('d'))
  expect(lines).toEqual(['M0,30 C30,30 30,16 60,16 S90,2 120,2', 'M0,23 C60,23 60,9 120,9'])
  // Blue for CPU, teal for memory, as the concept has them.
  expect([...container.querySelectorAll('svg')].map((svg) => svg.classList.contains('text-blue'))).toEqual([true, false])
  expect(container.querySelectorAll('svg')[1]).toHaveClass('text-teal')
})

test('with no minute behind it yet, a figure is its own level; one the host did not give draws nothing', () => {
  const { container } = render(HostCard, { host: web01, info: stats(50, null, 95) })
  const [cpu, mem] = container.querySelectorAll('svg')
  expect(cpu.querySelector('path[stroke]')).toHaveAttribute('d', 'M0,16 L120,16')
  expect(mem.querySelectorAll('path')).toHaveLength(0)
})

test('a host that is down can be asked again from its card; one that answers has nothing to try', async () => {
  const onretry = vi.fn()
  const { unmount } = render(HostCard, { host: web01, info: { ms: null, error: errors[2] as ConnError, shown: shown.en[2] as Shown }, onretry })
  await userEvent.click(screen.getByRole('button', { name: 'try again' }))
  expect(onretry).toHaveBeenCalledOnce()
  unmount()
  render(HostCard, { host: web01, info: { ms: 12, peek }, onretry })
  expect(screen.queryByRole('button', { name: 'try again' })).not.toBeInTheDocument()
})

test('the address is in the monospace face, the name is not', () => {
  const el = card(stats(100, null, null))
  expect(within(el).getByText('deploy@10.0.0.5:2222')).toHaveClass('font-mono')
  expect(within(el).getByRole('heading', { name: 'web-01' })).not.toHaveClass('font-mono')
})

test('a button says its key in its tooltip; a host nobody asked has a button for its quick peek, not a letter in a sentence', async () => {
  const onpeek = vi.fn()
  render(HostCard, { host: web01, info: { ms: 0, peek: null }, onconnect: vi.fn(), onfiles: vi.fn(), onforwards: vi.fn(), onedit: vi.fn(), onpeek })
  const el = screen.getByRole('article', { name: 'web-01' })
  expect(el.querySelector('kbd')).toBeNull()
  expect(within(el).getAllByRole('button').map((button) => button.title)).toEqual(['Connect (Enter)', 'Quick peek (V)'])
  await userEvent.click(within(el).getByRole('button', { name: 'Quick peek' }))
  expect(onpeek).toHaveBeenCalledOnce()
  setLang('vi')
  expect(await within(el).findByRole('button', { name: 'Xem nhanh' })).toHaveAttribute('title', 'Xem nhanh (V)')
  setLang('en')
  // A card that is tall has the three over its tags as well.
  render(HostCard, { host: bare, info: stats(100, null, null), onconnect: vi.fn(), onfiles: vi.fn(), onforwards: vi.fn(), onedit: vi.fn() })
  const tall = screen.getByRole('article', { name: 'bare' })
  expect(within(tall).getAllByRole('button').map((button) => button.title)).toEqual(['Edit (E)', 'Forwards (L)', 'Files (F)', 'Connect (Enter)'])
})

// --- D14: a host that is not watched

test('a card has its own switch, on until it is turned off; off, it keeps what was last heard and says that watching stopped', async () => {
  const user = userEvent.setup()
  const onwatch = vi.fn()
  const { rerender } = render(HostCard, { host: web01, info: { ms: 12, peek }, onwatch })
  const el = screen.getByRole('article', { name: 'web-01' })
  const watch = within(el).getByRole('switch', { name: 'Watch' })
  expect(watch).toBeChecked()
  expect(el).not.toHaveTextContent('watching stopped')
  await user.click(watch)
  // Asked for, not done: the window says it to the shell first.
  expect(onwatch).toHaveBeenCalledExactlyOnceWith(false)
  expect(watch).toBeChecked()

  await rerender({ host: { ...web01, meta: { ...web01.meta, paused: true } }, info: { ms: 12, peek }, onwatch })
  expect(watch).not.toBeChecked()
  // The figures of the last round stay; where the latency was, the card says nobody is asking.
  expect(el).toHaveTextContent('watching stopped')
  expect(el).not.toHaveTextContent('12 ms')
  expect(meter('CPU')).toHaveAttribute('aria-valuenow', '23')
  expect(el).toHaveAttribute('data-state', 'ok')
  await user.click(watch)
  expect(onwatch).toHaveBeenLastCalledWith(true)
})

test('a host that was never asked and is not watched is not "asking": it offers the quick peek; when no host is watched its own switch stays as it is', () => {
  const onpeek = vi.fn()
  render(HostCard, { host: { ...bare, meta: { ...bare.meta, paused: true } }, onwatch: vi.fn(), onpeek })
  const el = screen.getByRole('article', { name: 'bare' })
  expect(el).toHaveAttribute('data-state', 'unchecked')
  expect(el).not.toHaveTextContent('Asking bare')
  expect(within(el).getByRole('button', { name: 'Quick peek' })).toBeInTheDocument()

  // Every host at once: the card says so, and its own switch is still on for when the watching is back.
  render(HostCard, { host: web01, info: { ms: 12, peek }, onwatch: vi.fn(), paused: true })
  const all = screen.getByRole('article', { name: 'web-01' })
  expect(all).toHaveTextContent('watching stopped')
  expect(within(all).getByRole('switch', { name: 'Watch' })).toBeChecked()

  // Without a way to switch there is no switch.
  render(HostCard, { host: hosts[1] as Host, info: { ms: 12, peek } })
  expect(within(screen.getByRole('article', { name: 'web-02' })).queryByRole('switch')).toBeNull()
})

test('in Vietnamese the switch and what it says are Vietnamese', () => {
  setLang('vi')
  render(HostCard, { host: { ...web01, meta: { ...web01.meta, paused: true } }, info: { ms: 12, peek }, onwatch: vi.fn() })
  const el = screen.getByRole('article', { name: 'web-01' })
  expect(within(el).getByRole('switch', { name: 'Theo dõi' })).not.toBeChecked()
  expect(el).toHaveTextContent('đã ngừng theo dõi')
})
