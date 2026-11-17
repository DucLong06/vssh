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
