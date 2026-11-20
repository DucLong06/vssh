// How big the window draws itself, in percent of the size it was made at.
// It is the root's letter: every length of the window is a rem (app.css), so
// all of it follows, and a vh or a pointer's place means what it meant. The
// terminal's own letters are xterm's and stay; its room changes, and it is
// fitted to it again (Terminal.svelte).
export const STEPS = [80, 90, 100, 110, 125, 150] as const

/** The step nearest to what was asked for; what is no number is 100. */
const near = (percent: number) => STEPS.reduce<number>((best, step) => (Math.abs(step - percent) < Math.abs(best - percent) ? step : best), 100)

const saved = () => near(parseInt(localStorage.getItem('vssh.ui_scale') ?? '', 10))

export const scale = $state({ percent: saved() })

export function setScale(percent: number) {
  scale.percent = near(percent)
  localStorage.setItem('vssh.ui_scale', String(scale.percent))
  document.documentElement.style.fontSize = `${scale.percent}%`
}

/** One step bigger or smaller; at either end it stays. */
export const zoom = (by: 1 | -1) => setScale(STEPS[Math.max(0, Math.min(STEPS.length - 1, STEPS.indexOf(scale.percent as (typeof STEPS)[number]) + by))])

/** A length of the window as it was made, in px, as it is now: for what is placed by hand (a menu). */
export const scaled = (px: number) => (px * scale.percent) / 100

setScale(scale.percent)
