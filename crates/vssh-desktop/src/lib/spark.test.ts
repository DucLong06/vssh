import { render } from '@testing-library/svelte'
import { expect, test } from 'vitest'
import { POINTS, spark } from './spark'
import Sparkline from './Sparkline.svelte'

/** A height in the concept's box as the percentage that is drawn there. */
const pct = (y: number) => (30 - y) / 0.28

test("the concept's own sparkline, letter for letter: nine samples, level at each, edge to edge", () => {
  // docs/mockups/concept-a.html, #sp-low.
  const { line, area } = spark([25, 26, 24, 26, 23, 25, 24, 26, 25].map(pct))
  expect(line).toBe('M0,25 C7.5,25 7.5,26 15,26 S22.5,24 30,24 S37.5,26 45,26 S52.5,23 60,23 S67.5,25 75,25 S82.5,24 90,24 S97.5,26 105,26 S112.5,25 120,25')
  expect(area).toBe(`${line} L120,32 L0,32 Z`)
})

test('a full minute is twelve points: eleven curves from the left edge to the right', () => {
  const { line } = spark([50, 50, 100, 0, 50, 50, 50, 50, 50, 50, 50, 50])
  expect(line.startsWith('M0,16 C5.45,16 5.45,16 10.91,16 S16.36,2 21.82,2 S27.27,30 32.73,30 ')).toBe(true)
  expect(line.endsWith(' 120,16')).toBe(true)
  expect(line.match(/[CS]/g)).toHaveLength(POINTS - 1)
})

test('fewer than twelve points are drawn from edge to edge all the same', () => {
  expect(spark([0, 50, 100]).line).toBe('M0,30 C30,30 30,16 60,16 S90,2 120,2')
  expect(spark([25, 75]).line).toBe('M0,23 C60,23 60,9 120,9')
})

test('one point is its level, straight across; none is nothing', () => {
  expect(spark([50])).toEqual({ line: 'M0,16 L120,16', area: 'M0,16 L120,16 L120,32 L0,32 Z' })
  expect(spark([])).toEqual({ line: '', area: '' })
})

test('only the last twelve are drawn, and a figure past its scale stays in the box', () => {
  const rounds = Array.from({ length: 20 }, (_, at) => at * 5)
  expect(spark(rounds)).toEqual(spark(rounds.slice(-POINTS)))
  expect(spark([250, -5]).line).toBe('M0,2 C60,2 60,30 120,30')
})

test('the component draws the line over its area, and nothing for a host that gave no figure', () => {
  const { container, unmount } = render(Sparkline, { points: [0, 50, 100], class: 'text-blue' })
  const svg = container.querySelector('svg')!
  // A picture: the figure is said beside it.
  expect(svg).toHaveAttribute('aria-hidden', 'true')
  expect(svg).toHaveAttribute('viewBox', '0 0 120 32')
  expect(svg).toHaveClass('text-blue')
  const [area, line] = [...svg.querySelectorAll('path')].map((path) => path.getAttribute('d'))
  expect(line).toBe('M0,30 C30,30 30,16 60,16 S90,2 120,2')
  expect(area).toBe(`${line} L120,32 L0,32 Z`)
  unmount()
  expect(render(Sparkline, { points: [] }).container.querySelectorAll('path')).toHaveLength(0)
})
