// A host's figures as the cards show them. The numbers come from the core's
// parser; the wording and the thresholds here are `Stats::parts` of
// `vssh_core::stats` again, held to its test's examples by `stats.test.ts`.
import type { Lang, Stats } from './api'

export type Level = 'ok' | 'warn' | 'crit'

export type Part = {
  label: string
  value: string
  /** How full the bar is: 0 to 100. */
  pct: number
  level: Level
}

const GIB = 1024 * 1024

/** Every metric the host answered, most important first. */
export function parts(stats: Stats, mem: string): Part[] {
  const out: Part[] = []
  const add = (label: string, value: string, pct: number, warn: number, crit: number) =>
    out.push({ label, value, pct: Math.min(pct, 100), level: pct > crit ? 'crit' : pct > warn ? 'warn' : 'ok' })
  if (stats.cpu !== null) add('CPU', `${Math.min(stats.cpu, 999)}%`, stats.cpu, 50, 80)
  if (stats.mem) {
    const [used, total] = stats.mem
    // A small machine keeps the decimal: 0.5G, not 0G or 1G.
    const decimals = total / GIB < 10 ? 1 : 0
    const value = `${(used / GIB).toFixed(1)}/${(total / GIB).toFixed(decimals)}G`
    add(mem, value, Math.floor((used * 100) / Math.max(total, 1)), 70, 85)
  }
  if (stats.disk !== null) add('/', `${stats.disk}%`, stats.disk, 80, 90)
  return out
}

const UNITS: Record<string, string> = { day: 'ngày', hr: 'giờ', min: 'phút', sec: 'giây' }

/** The host's `uptime` words in this language, as `vssh_core::stats::uptime`: `41 ngày, 3 phút`. */
export const uptime = (up: string, lang: Lang) =>
  lang === 'vi' ? up.replace(/\b(day|hr|min|sec)s?\b/g, (_, unit: string) => UNITS[unit]) : up

/** `CPU 23% · MEM 4.2/16G · / 61%` */
export const compact = (list: Part[]) => list.map((part) => `${part.label} ${part.value}`).join(' · ')
