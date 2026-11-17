// What the dashboard knows about each host right now, and the asking that
// keeps it current. Everything here answers into `live`; nothing rejects.
import { api, explain, type ConnError, type Lang, type Peek, type Shown } from './api'
import { i18n } from './i18n.svelte'
import { POINTS } from './spark'
import { parts } from './stats'

/** One host. A field that is absent has not been answered yet. */
export type Live = {
  /** TCP connect time in ms; `null` = unreachable. */
  ms?: number | null
  /** The last round of metrics; `null` = the dashboard may not ask on its own. */
  peek?: Peek | null
  /** Why the last round failed: as it came, and in the window's language. */
  error?: ConnError
  shown?: Shown
  /** When `peek` or `error` arrived, in ms since the epoch. */
  at?: number
  asking?: boolean
}

export const live = $state<Record<string, Live>>({})

/** A host's last rounds of CPU and memory, in percent, oldest first: what its sparklines draw. */
export type Trail = { cpu: number[]; mem: number[] }

export const trail = $state<Record<string, Trail>>({})

/** Forgets every host. */
export function forget() {
  for (const kept of [live, trail]) Object.keys(kept).forEach((alias) => delete kept[alias])
}

/** One more round of a host's trail; a figure it did not give leaves no point. */
function follow(alias: string, peek: Peek) {
  // Read back, not kept from the assignment: that would be the bare object, which nothing watches.
  trail[alias] ??= { cpu: [], mem: [] }
  const kept = trail[alias]
  const { cpu, mem } = peek.stats
  const add = (list: number[], pct: number) => list.push(pct) > POINTS && list.shift()
  if (cpu !== null) add(kept.cpu, Math.min(cpu, 100))
  if (mem) add(kept.mem, Math.floor((mem[0] * 100) / Math.max(mem[1], 1)))
}

/**
 * One round of metrics. `passive` is the dashboard's own poll: the shell
 * answers it with `null` for a host whose key it has never seen or that sits
 * behind a jump host. Without it, this is a person asking, as `v` in the TUI.
 */
export async function ask(alias: string, passive: boolean) {
  live[alias] ??= {}
  const host = live[alias]
  if (host.asking) return
  host.asking = true
  try {
    const peek = await api.getHostStats(alias, passive)
    // "Not asked" says nothing new about a host a person did ask about.
    if (peek || (!host.peek && !host.error)) {
      Object.assign(host, { peek, error: undefined, shown: undefined, at: Date.now() })
    }
    if (peek) follow(alias, peek)
  } catch (error) {
    const shown = await explain(error, i18n.lang)
    Object.assign(host, { peek: undefined, error: error as ConnError, shown, at: Date.now() })
    // What it was doing before it went is no part of what it does when it is back.
    delete trail[alias]
  } finally {
    host.asking = false
  }
}

export async function ping(alias: string) {
  // Refused for a host that is not reached directly: no answer, not "down".
  const ms = await api.pingHost(alias).catch(() => undefined)
  live[alias] ??= {}
  live[alias].ms = ms
}

/** One round of the dashboard over every host. A window nobody sees asks nobody. */
export function poll(aliases: string[], pings: boolean) {
  if (document.hidden) return
  for (const alias of aliases) {
    void ask(alias, true)
    if (pings) void ping(alias)
  }
}

/** Puts every failure into the words of another language. */
export async function relabel(lang: Lang) {
  for (const host of Object.values(live)) {
    if (host.error) host.shown = await explain(host.error, lang)
  }
}

/** Past its critical mark somewhere. */
export const critical = (host: Live | undefined) =>
  !!host?.peek && parts(host.peek.stats, '').some((part) => part.level === 'crit')

/** Does not answer: its last round failed, or nobody asked and a ping got nowhere. */
export const down = (host: Live | undefined) => !!host && (!!host.error || (host.ms === null && !host.peek))

/** Answers: its figures came, or a ping got through and nothing failed since. */
export const up = (host: Live | undefined) => !down(host) && (!!host?.peek || typeof host?.ms === 'number')

/** A host that needs looking at. */
export const sick = (host: Live | undefined) => down(host) || critical(host)
