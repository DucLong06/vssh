// A sparkline as Concept A draws it (docs/mockups/concept-a.html): a smooth
// line through the points in a box of 120 by 32, and the area under it.

/** How many rounds a sparkline holds: twelve of the dashboard's five seconds, a minute. */
export const POINTS = 12
const [WIDTH, HEIGHT] = [120, 32]
const n = (value: number) => String(+value.toFixed(2))

/**
 * The paths of `points`, percentages, oldest first, drawn from edge to edge
 * however many there are: a host asked only a few times has a line of few
 * bends, not a stub in a corner. One point is its level, straight across;
 * none is nothing.
 */
export function spark(points: number[]): { line: string; area: string } {
  const shown = points.slice(-POINTS)
  if (!shown.length) return { line: '', area: '' }
  // The concept's scale: 0% two units above the floor, 100% two below the ceiling.
  const y = (pct: number) => n(30 - Math.min(Math.max(pct, 0), 100) * 0.28)
  const floor = ` L${WIDTH},${HEIGHT} L0,${HEIGHT} Z`
  if (shown.length === 1) {
    const line = `M0,${y(shown[0])} L${WIDTH},${y(shown[0])}`
    return { line, area: line + floor }
  }
  const step = WIDTH / (shown.length - 1)
  // Level at every point: the first curve names both its handles, each later one mirrors the last.
  const curves = shown.slice(1).map((pct, at) => {
    const mid = n((at + 0.5) * step)
    const end = `${mid},${y(pct)} ${n((at + 1) * step)},${y(pct)}`
    return at === 0 ? `C${mid},${y(shown[0])} ${end}` : `S${end}`
  })
  const line = `M0,${y(shown[0])} ${curves.join(' ')}`
  return { line, area: line + floor }
}
