import type { Host } from './api'

/** `user@hostname:port`, as much of it as the ssh config gives. */
export function address(host: Host): string {
  const user = host.user ? `${host.user}@` : ''
  const port = host.port ? `:${host.port}` : ''
  return host.hostname ? `${user}${host.hostname}${port}` : ''
}

/** A host's tags and its group, as the TUI counts them: one namespace. */
export function tagsOf(host: Host): string[] {
  return [...host.meta.tags.split(/[ ,]/), host.meta.grp].filter(Boolean)
}

/** Where a host's card goes: its group, else its first tag, else nowhere (''). */
export const groupOf = (host: Host) => host.meta.grp || tagsOf(host)[0] || ''

export const hasTag = (host: Host, tag: string) =>
  tagsOf(host).some((name) => name.toLowerCase() === tag.toLowerCase())

/** Every tag with the number of hosts under it: most used first, then by name. */
export function tagCounts(hosts: Host[]): [name: string, count: number][] {
  const names = new Map<string, string>()
  for (const name of hosts.flatMap(tagsOf)) {
    if (!names.has(name.toLowerCase())) names.set(name.toLowerCase(), name)
  }
  const count = (tag: string) => hosts.filter((host) => hasTag(host, tag)).length
  return [...names.values()]
    .map((name): [string, number] => [name, count(name)])
    .sort(([a, na], [b, nb]) => nb - na || a.localeCompare(b))
}

const CHIPS = ['blue', 'magenta', 'cyan', 'orange', 'teal', 'yellow']

/** A tag's colour: the same one every time, hashed as the TUI hashes its badges. */
export function chip(tag: string): string {
  let hash = 0
  for (const letter of tag) hash = (hash * 31 + letter.charCodeAt(0)) >>> 0
  return `var(--color-${CHIPS[hash % CHIPS.length]})`
}
