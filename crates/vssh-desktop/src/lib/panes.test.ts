import { expect, test } from 'vitest'
import { bars, half, ids, none, place, rects, mapped, resize, row, shift, without, zone } from './panes'

const shape = (layout: ReturnType<typeof none>) => [layout.stacked ? 'stacked' : 'beside', layout.groups.map((group) => group.panes)]

test('a tab let go at an edge of a tab is a split of the two, on that side', () => {
  expect(shape(place(none(), 2, 'right', 1))).toEqual(['beside', [[1], [2]]])
  expect(shape(place(none(), 2, 'left', 1))).toEqual(['beside', [[2], [1]]])
  expect(shape(place(none(), 2, 'bottom', 1))).toEqual(['stacked', [[1], [2]]])
  expect(shape(place(none(), 2, 'top', 1))).toEqual(['stacked', [[2], [1]]])
  // On itself: nothing to be beside.
  expect(place(none(), 1, 'right', 1)).toEqual(none())
})

test('nested: one pane on the left, two on the right, one over the other; and a grid', () => {
  const two = place(none(), 2, 'right', 1)
  const three = place(two, 3, 'bottom', 2)
  expect(shape(three)).toEqual(['beside', [[1], [2, 3]]])
  expect(rects(three)).toEqual({ 1: { x: 0, y: 0, w: 0.5, h: 1 }, 2: { x: 0.5, y: 0, w: 0.5, h: 0.5 }, 3: { x: 0.5, y: 0.5, w: 0.5, h: 0.5 } })
  const grid = place(three, 4, 'top', 1)
  expect(shape(grid)).toEqual(['beside', [[4, 1], [2, 3]]])
  // Along the way the groups run, a pane gets a group of its own.
  expect(shape(place(three, 4, 'left', 3))).toEqual(['beside', [[1], [4], [2, 3]]])
  // The same the other way up.
  expect(shape(place(place(none(), 2, 'bottom', 1), 3, 'right', 2))).toEqual(['stacked', [[1], [2, 3]]])
})

test('a pane moved to another edge leaves where it was; in the middle of another the two change places', () => {
  const two = row([1, 2])
  expect(shape(place(two, 1, 'right', 2))).toEqual(['beside', [[2], [1]]])
  expect(shape(place(two, 1, 'bottom', 2))).toEqual(['stacked', [[2], [1]]])
  const three = place(two, 3, 'bottom', 2)
  expect(shape(place(three, 1, 'centre', 3))).toEqual(['beside', [[3], [2, 1]]])
  // A tab that was in no split takes the pane's place, and the pane is a tab again.
  expect(shape(place(three, 9, 'centre', 2))).toEqual(['beside', [[1], [9, 3]]])
  expect(shape(shift(three, 1, 1))).toEqual(['beside', [[2], [1, 3]]])
  expect(shift(three, 1, -1)).toBe(three)
})

test('a pane that goes: its group closes up; two that are left of a group are the split; one alone is no split', () => {
  const three = place(row([1, 2]), 3, 'bottom', 2)
  expect(shape(without(three, 1))).toEqual(['stacked', [[2], [3]]])
  expect(shape(without(three, 3))).toEqual(['beside', [[1], [2]]])
  expect(without(row([1, 2]), 2)).toEqual(none())
  expect(ids(three)).toEqual([1, 2, 3])
})

test('the lines between panes can be moved, each as far as leaves both sides something', () => {
  const three = place(row([1, 2]), 3, 'bottom', 2)
  const [between, inside] = bars(three)
  expect(between).toMatchObject({ group: true, upright: true, rect: { x: 0.5, y: 0, w: 0, h: 1 } })
  expect(inside).toMatchObject({ group: false, upright: false, rect: { x: 0.5, y: 0.5, w: 0.5, h: 0 } })
  const wider = resize(three, between, 0.2)
  expect(rects(wider)[1].w).toBeCloseTo(0.7)
  expect(rects(wider)[2]).toMatchObject({ x: 0.7, h: 0.5 })
  const taller = resize(wider, bars(wider)[1], -0.25)
  expect(rects(taller)[2].h).toBeCloseTo(0.25)
  expect(rects(taller)[3].h).toBeCloseTo(0.75)
  expect(rects(taller)[1].w).toBeCloseTo(0.7)
  // Never to nothing.
  expect(rects(resize(three, between, 5))[2].w).toBeCloseTo(0.12)
  expect(rects(resize(three, between, -5))[1].w).toBeCloseTo(0.12)
  // A size goes with its pane when the pane goes.
  expect(without(taller, 1).size).toEqual({})
})

test('where a tab in hand is: the pane under it, an edge of it within three tenths, else its middle', () => {
  const where = rects(row([1, 2]))
  expect(zone(where, 0.05, 0.5)).toEqual({ id: 1, zone: 'left' })
  expect(zone(where, 0.25, 0.5)).toEqual({ id: 1, zone: 'centre' })
  expect(zone(where, 0.45, 0.5)).toEqual({ id: 1, zone: 'right' })
  expect(zone(where, 0.75, 0.05)).toEqual({ id: 2, zone: 'top' })
  expect(zone(where, 0.95, 0.5)).toEqual({ id: 2, zone: 'right' })
  expect(zone(where, 1.2, 0.5)).toBeNull()
  expect(half(where[2], 'right')).toEqual({ x: 0.75, y: 0, w: 0.25, h: 1 })
  expect(half(where[2], 'centre')).toEqual(where[2])
})

test('a split under other names for its panes: the shape and the sizes go with them, and a pane with no name is left out', () => {
  const three = resize(place(row([1, 2]), 3, 'bottom', 2), bars(place(row([1, 2]), 3, 'bottom', 2))[1], 0.25)
  const named = mapped(three, (id) => id * 10)
  expect(shape(named)).toEqual(['beside', [[10], [20, 30]]])
  expect(rects(named)[20].h).toBeCloseTo(0.75)
  expect(rects(mapped(named, (id) => id / 10))).toEqual(rects(three))
  // Without one of the two on the right, the other has the side to itself.
  expect(shape(mapped(three, (id) => (id === 3 ? undefined : id)))).toEqual(['beside', [[1], [2]]])
  expect(mapped(row([1, 2]), (id) => (id === 1 ? undefined : id))).toEqual(none())
})
