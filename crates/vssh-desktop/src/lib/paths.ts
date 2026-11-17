// Paths and figures of the file view. Paths are absolute and joined with
// `/` on both sides: the hosts are Unix, and so is whatever runs the TUI.

export const join = (dir: string, name: string) => `${dir.replace(/\/+$/, '')}/${name}`

export function parent(dir: string): string {
  const at = dir.replace(/\/+$/, '').lastIndexOf('/')
  return at <= 0 ? '/' : dir.slice(0, at)
}

/** Every directory on the way to `path`, the root first. */
export function crumbs(path: string): { name: string; path: string }[] {
  if (!path) return []
  const names = path.split('/').filter(Boolean)
  return [{ name: '/', path: '/' }, ...names.map((name, at) => ({ name, path: `/${names.slice(0, at + 1).join('/')}` }))]
}

/** A size as the TUI writes it: `1023 B`, `1.5 K`. */
export function human(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  let n = bytes
  for (const unit of ['K', 'M', 'G']) if ((n /= 1024) < 1024) return `${n.toFixed(1)} ${unit}`
  return `${(n / 1024).toFixed(1)} T`
}

const two = (n: number) => String(n).padStart(2, '0')

/** Unix seconds as `2025-11-16 09:05`, local time; 0 is a time nobody knows. */
export function when(mtime: number): string {
  if (!mtime) return '—'
  const at = new Date(mtime * 1000)
  return `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())} ${two(at.getHours())}:${two(at.getMinutes())}`
}

/** Seconds still to go: `42s`, `3m05s`, `1h02m`. */
export function eta(secs: number): string {
  if (!Number.isFinite(secs)) return '—'
  const s = Math.ceil(secs)
  if (s < 60) return `${s}s`
  return s < 3600 ? `${Math.floor(s / 60)}m${two(s % 60)}s` : `${Math.floor(s / 3600)}h${two(Math.floor((s % 3600) / 60))}m`
}
