// The tabs that are open together, and where each is. Two levels, which is a
// row of columns or a column of rows: the groups run one way (side by side, or
// `stacked` one over the other), the panes of a group the other way. So: one
// pane on the left and two on the right, one over the other; or a grid.
//
// shortcut: two levels, not a tree: a pane inside a group cannot be divided
// again along the groups' own direction. Make `Group` recursive if that is wanted.

export type Edge = 'left' | 'right' | 'top' | 'bottom'
/** Where in a pane something is let go: at an edge of it, or in its middle. */
export type Zone = Edge | 'centre'
export type Group = { panes: number[]; size: number }
/** `size`: a pane's share of its group, where it is not 1. A group's share of the whole is its own. */
export type Layout = { stacked: boolean; groups: Group[]; size: Record<number, number> }
/** A part of the whole, each side from 0 to 1. */
export type Rect = { x: number; y: number; w: number; h: number }
/** A line between two panes that can be moved: `group` between two groups, else between two panes of group `g`. */
export type Bar = { g: number; at: number; group: boolean; rect: Rect; upright: boolean }

/** No pane may be made smaller than this share of what it is in. */
const LEAST = 0.12

export const none = (): Layout => ({ stacked: false, groups: [], size: {} })

/** The panes, in reading order. */
export const ids = (layout: Layout) => layout.groups.flatMap((group) => group.panes)

const share = (layout: Layout, id: number) => layout.size[id] ?? 1

/** As it is kept: no empty group, one pane alone is no split, and a lone group's panes are groups themselves. */
function tidy(layout: Layout): Layout {
  let groups = layout.groups.filter((group) => group.panes.length)
  let stacked = layout.stacked
  if (groups.length === 1 && groups[0].panes.length > 1) {
    stacked = !stacked
    groups = groups[0].panes.map((id) => ({ panes: [id], size: share(layout, id) }))
  }
  const all = groups.flatMap((group) => group.panes)
  if (all.length < 2) return none()
  // A pane alone in its group fills it: its share is the group's.
  const alone = new Set(groups.filter((group) => group.panes.length === 1).map((group) => group.panes[0]))
  const size = Object.fromEntries(Object.entries(layout.size).filter(([id]) => all.includes(Number(id)) && !alone.has(Number(id))))
  return { stacked, groups, size }
}

/** Without this pane; what is left of a split of two is no split. */
export function without(layout: Layout, id: number): Layout {
  return tidy({ ...layout, groups: layout.groups.map((group) => ({ ...group, panes: group.panes.filter((pane) => pane !== id) })) })
}

/** The two change places; one that is in no split takes the other's place in it. */
function exchange(layout: Layout, a: number, b: number): Layout {
  const other = (id: number) => (id === a ? b : id === b ? a : id)
  const has = ids(layout)
  // Only one of them is a pane: it goes, and the other is where it was.
  const swap = has.includes(a) && has.includes(b) ? other : (id: number) => (id === b ? a : id)
  return tidy({ ...layout, groups: layout.groups.map((group) => ({ ...group, panes: group.panes.map(swap) })) })
}

/** Puts a tab at an edge of a pane, or (`centre`) in its place. `beside` alone is a split of one, about to be two. */
export function place(layout: Layout, id: number, zone: Zone, beside: number): Layout {
  if (id === beside) return layout
  if (zone === 'centre') return exchange(layout, id, beside)
  let next = without(layout, id)
  if (!ids(next).includes(beside)) next = { stacked: zone === 'top' || zone === 'bottom', groups: [{ panes: [beside], size: 1 }], size: {} }
  const before = zone === 'left' || zone === 'top'
  const g = next.groups.findIndex((group) => group.panes.includes(beside))
  const groups = next.groups.map((group) => ({ ...group, panes: [...group.panes] }))
  // Along the way the groups run: a group of its own. Across it: into the pane's group.
  if ((zone === 'top' || zone === 'bottom') === next.stacked) {
    groups.splice(before ? g : g + 1, 0, { panes: [id], size: 1 })
  } else {
    const at = groups[g].panes.indexOf(beside)
    groups[g].panes.splice(before ? at : at + 1, 0, id)
  }
  return tidy({ ...next, groups })
}

/** A pane changes places with the one before it in reading order, or after. */
export function shift(layout: Layout, id: number, by: number): Layout {
  const all = ids(layout)
  const other = all[all.indexOf(id) + by]
  return all.includes(id) && other !== undefined ? exchange(layout, id, other) : layout
}

/** The same split under other names for its panes; a pane with no name there is left out. */
export function mapped(layout: Layout, to: (id: number) => number | undefined): Layout {
  const kept = ids(layout).reduce((left, id) => (to(id) === undefined ? without(left, id) : left), layout)
  return {
    stacked: kept.stacked,
    groups: kept.groups.map((group) => ({ ...group, panes: group.panes.map((id) => to(id)!) })),
    size: Object.fromEntries(Object.entries(kept.size).map(([id, share]) => [to(Number(id)), share])),
  }
}

/** A split of these panes side by side, or one over the other. */
export const row = (panes: number[], stacked = false): Layout => tidy({ stacked, groups: panes.map((id) => ({ panes: [id], size: 1 })), size: {} })

/** Where each pane is, as parts of the whole. */
export function rects(layout: Layout): Record<number, Rect> {
  const out: Record<number, Rect> = {}
  const total = layout.groups.reduce((sum, group) => sum + group.size, 0)
  let main = 0
  for (const group of layout.groups) {
    const length = group.size / total
    const sum = group.panes.reduce((all, id) => all + share(layout, id), 0)
    let cross = 0
    for (const id of group.panes) {
      const part = share(layout, id) / sum
      out[id] = layout.stacked ? { x: cross, y: main, w: part, h: length } : { x: main, y: cross, w: length, h: part }
      cross += part
    }
    main += length
  }
  return out
}

/** The lines that can be moved: between two groups, and between two panes of a group. */
export function bars(layout: Layout): Bar[] {
  const where = rects(layout)
  const out: Bar[] = []
  layout.groups.forEach((group, g) => {
    const first = where[group.panes[0]]
    if (g > 0) {
      // Before this group, across everything.
      out.push({ g: g - 1, at: 0, group: true, upright: !layout.stacked, rect: layout.stacked ? { x: 0, y: first.y, w: 1, h: 0 } : { x: first.x, y: 0, w: 0, h: 1 } })
    }
    group.panes.slice(1).forEach((id, at) => {
      const r = where[id]
      out.push({ g, at, group: false, upright: layout.stacked, rect: layout.stacked ? { x: r.x, y: r.y, w: 0, h: r.h } : { x: r.x, y: r.y, w: r.w, h: 0 } })
    })
  })
  return out
}

/** Moves a line by a part of the whole: what is before it grows by that, what is after it gives it. */
export function resize(layout: Layout, bar: Bar, by: number): Layout {
  const groups = layout.groups.map((group) => ({ ...group }))
  const size = { ...layout.size }
  // The two shares on either side, the sum they keep, and how much of the whole that sum is.
  const move = (a: number, b: number, whole: number, set: (a: number, b: number) => void) => {
    const sum = a + b
    const least = Math.min(LEAST * whole, sum / 2)
    const next = Math.min(Math.max(a + by * whole, least), sum - least)
    set(next, sum - next)
  }
  if (bar.group) {
    const total = groups.reduce((sum, group) => sum + group.size, 0)
    move(groups[bar.g].size, groups[bar.g + 1].size, total, (a, b) => {
      groups[bar.g].size = a
      groups[bar.g + 1].size = b
    })
  } else {
    const [a, b] = [groups[bar.g].panes[bar.at], groups[bar.g].panes[bar.at + 1]]
    const sum = groups[bar.g].panes.reduce((all, id) => all + share(layout, id), 0)
    move(share(layout, a), share(layout, b), sum, (x, y) => {
      size[a] = x
      size[b] = y
    })
  }
  return { ...layout, groups, size }
}

/** The pane a point is in, and where in it: near an edge, or in the middle. */
export function zone(where: Record<number, Rect>, x: number, y: number): { id: number; zone: Zone } | null {
  for (const [id, r] of Object.entries(where)) {
    const [px, py] = [(x - r.x) / r.w, (y - r.y) / r.h]
    if (px < 0 || px > 1 || py < 0 || py > 1) continue
    const far: [number, Edge][] = [[px, 'left'], [1 - px, 'right'], [py, 'top'], [1 - py, 'bottom']]
    const [near, edge] = far.sort((a, b) => a[0] - b[0])[0]
    return { id: Number(id), zone: near < 0.3 ? edge : 'centre' }
  }
  return null
}

/** The part of a pane a tab let go in `zone` of it would take. */
export function half(r: Rect, zone: Zone): Rect {
  if (zone === 'left') return { ...r, w: r.w / 2 }
  if (zone === 'right') return { ...r, x: r.x + r.w / 2, w: r.w / 2 }
  if (zone === 'top') return { ...r, h: r.h / 2 }
  if (zone === 'bottom') return { ...r, y: r.y + r.h / 2, h: r.h / 2 }
  return r
}
