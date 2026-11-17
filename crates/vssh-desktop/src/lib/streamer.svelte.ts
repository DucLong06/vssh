// Streamer mode: every address the window shows is a made-up one, so the
// window can be on a stream. Only what is shown: the config, the database and
// what is sent to the shell keep the real ones. What a shell prints is the
// user's own and is left alone.
import type { Host } from './api'
import { address, fromTarget } from './hosts'

const saved = () => localStorage.getItem('vssh.streamer_mode') === 'true'

/** `fake`: the hosts' made-up addresses, by alias. */
export const streamer = $state({ on: saved(), fake: {} as Record<string, string> })

export function setStreamer(on: boolean) {
  streamer.on = on
  localStorage.setItem('vssh.streamer_mode', String(on))
}

/** The documentation ranges (RFC 5737): addresses that are nobody's. */
const NETS = ['203.0.113', '198.51.100']
const SLOTS = NETS.length * 254
const ip = (slot: number) => `${NETS[Math.floor(slot / 254)]}.${(slot % 254) + 1}`
const MADE_UP = /^(203\.0\.113|198\.51\.100)\./
const LOOPBACK = /^(localhost|127(\.\d+){3}|::1)$/

function slot(name: string) {
  let hash = 0
  for (const letter of name) hash = (hash * 31 + letter.charCodeAt(0)) >>> 0
  return hash % SLOTS
}

/** A host's real name or address, and the made-up one that stands for it in a failure's words. */
let known = new Map<string, string>()
let names: RegExp | null = null

/**
 * Gives every host an address of its own, the same for the same hosts: where
 * two aliases hash to one, the later host of the config takes the next free.
 */
export function learn(hosts: Host[]) {
  const taken = new Set<number>()
  const table: Record<string, string> = {}
  for (const { alias } of hosts) {
    let at = slot(alias)
    // More hosts than addresses: the last ones share.
    for (let tries = 0; taken.has(at) && tries < SLOTS; tries++) at = (at + 1) % SLOTS
    taken.add(at)
    table[alias] = ip(at)
  }
  streamer.fake = table
  // Of two hosts at one address the first gives it its stand-in.
  known = new Map(hosts.filter((host) => host.hostname).map((host): [string, string] => [host.hostname!, table[host.alias]]).reverse())
  const escaped = [...known.keys()].sort((a, b) => b.length - a.length).map((name) => name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  // Whole names only: `db` is not in `db-primary`, nor 10.0.0.5 in 10.0.0.55.
  names = escaped.length ? new RegExp(`(^|[^\\w.-])(${escaped.join('|')})(?![\\w-]|\\.\\w)`, 'g') : null
}

/** The made-up address of a host, by its alias; of anything else, by what it is called. */
export const fake = (name: string) => streamer.fake[name] ?? ip(slot(name))

/** A name or an address that is no host of the config: where a forward leads, what a quick connect was typed as. This machine's own stays. */
export const hide = (name: string) => (streamer.on && !LOOPBACK.test(name) ? fake(name) : name)

/** A host's `user@hostname:port`, as the window may show it. */
export const addr = (host: Host) =>
  address(streamer.on && host.hostname ? { ...host, hostname: fake(host.alias) } : host)

/** A quick connect's `[user@]host[:port]`, as the window may show it. */
export function target(text: string): string {
  if (!streamer.on) return text
  const { hostname, user, port } = fromTarget(text)
  return `${user && `${user}@`}${hide(hostname)}${port ? `:${port}` : ''}`
}

/** Words that came with addresses in them, a failure's: the hosts' own become the ones their cards show, any other IPv4 a made-up one. */
// shortcut: an IPv6 address that is no host's own stays; match those too if a failure ever names one.
export function scrub(text: string): string {
  if (!streamer.on) return text
  const named = names ? text.replace(names, (_, before: string, name: string) => before + known.get(name)) : text
  // No lookbehind, here or above: a WebKit without it would not load the window at all.
  return named.replace(/(^|[^\d.])(\d{1,3}(?:\.\d{1,3}){3})(?!\d|\.\d)/g, (_, before: string, ip: string) => before + (MADE_UP.test(ip) ? ip : hide(ip)))
}
