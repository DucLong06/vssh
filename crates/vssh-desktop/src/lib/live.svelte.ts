// What the dashboard knows about each host right now, and the asking that
// keeps it current. Everything here answers into `live`; nothing rejects.
import { api, explain, type ConnError, type Lang, type Peek, type Shown } from './api'
import { i18n } from './i18n.svelte'
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

/** Forgets every host. */
export const forget = () => Object.keys(live).forEach((alias) => delete live[alias])

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
  } catch (error) {
    const shown = await explain(error, i18n.lang)
    Object.assign(host, { peek: undefined, error: error as ConnError, shown, at: Date.now() })
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
