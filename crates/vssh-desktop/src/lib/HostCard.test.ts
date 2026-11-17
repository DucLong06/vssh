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

test('a healthy host: name, address, tags, latency and three bars', () => {
  const el = card({ ms: 12, peek })
  expect(within(el).getByRole('heading', { name: 'web-01' })).toBeInTheDocument()
  expect(el).toHaveTextContent('deploy@10.0.0.5:2222')
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
  expect(el).toHaveTextContent('CPU — MEM — / —')
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

test('a card that can open the host\'s files has a button for it, next to Connect', async () => {
  const [onfiles, onconnect] = [vi.fn(), vi.fn()]
  render(HostCard, { host: web01, onfiles, onconnect })
  const buttons = screen.getAllByRole('button').map((button) => button.textContent?.trim())
  expect(buttons).toEqual(['Files', 'Connect'])
  await userEvent.click(screen.getByRole('button', { name: 'Files' }))
  expect(onfiles).toHaveBeenCalledOnce()
  expect(onconnect).not.toHaveBeenCalled()
})
