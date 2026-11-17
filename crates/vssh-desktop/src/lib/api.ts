// The shell's commands and the shapes of their answers. Written by hand:
// tauri-specta, which would generate this file, is still a release candidate.
// `src-tauri/tests/commands.rs` asserts these shapes on the Rust side and
// reads `fixtures/*.json`, so the two cannot drift apart unnoticed.
import { invoke } from '@tauri-apps/api/core'

export type Lang = 'en' | 'vi'

export type Meta = {
  grp: string
  /** Space or comma separated. */
  tags: string
  notes: string
  pinned: boolean
  /** Unix seconds; 0 = never. */
  last_connected: number
  connects: number
}

/** A host of `~/.ssh/config`. What the config does not say is `null`. */
export type Host = {
  alias: string
  hostname: string | null
  user: string | null
  port: number | null
  meta: Meta
}

export type Stats = {
  /** 1-minute load per core, in per cent. */
  cpu: number | null
  /** KiB. */
  mem: [used: number, total: number] | null
  /** How full `/` is, in per cent. */
  disk: number | null
}

export type Peek = {
  stats: Stats
  load: string | null
  uptime: string | null
  os: string | null
  host: string | null
}

export type Snippet = { id: number; host: string; name: string; command: string }

export type ErrorClass =
  | 'dns'
  | 'timeout'
  | 'refused'
  | 'unreachable'
  | 'auth'
  | 'host_key'
  | 'jump'
  | 'other'

export type Fail =
  | { kind: 'dns'; host: string }
  | { kind: 'timeout'; addr: string; secs: number }
  | { kind: 'refused'; addr: string }
  | { kind: 'unreachable'; addr: string }
  | { kind: 'auth'; user: string; host: string; keys: number; password: boolean }
  | { kind: 'host_key'; host: string; fingerprint: string; line: number; file: string }
  | { kind: 'jump'; hop: string; cause: Fail | null }

/** What a command rejects with: a failure in no language yet. */
export type ConnError = { class: ErrorClass; fail: Fail | null; chain: string[] }

/** A failure put into words. */
export type Shown = { class: ErrorClass; label: string; message: string; chain: string }

export const api = {
  listHosts: () => invoke<Host[]>('list_hosts'),
  getHostStats: (alias: string) => invoke<Peek>('get_host_stats', { alias }),
  /** `[user@]host[:port]`; resolves to the id of the new session. */
  quickConnect: (host: string) => invoke<number>('quick_connect', { host }),
  listSnippets: () => invoke<Snippet[]>('list_snippets'),
  resolveConnError: (error: ConnError, lang: Lang) =>
    invoke<Shown>('resolve_conn_error', { error, lang }),
}

/** Whatever a command rejected with, in words. Never rejects itself. */
export async function explain(error: unknown, lang: Lang): Promise<Shown> {
  try {
    return await api.resolveConnError(error as ConnError, lang)
  } catch {
    // Not a ConnError: Tauri's own words about a call it could not make.
    const words = String(error)
    return { class: 'other', label: 'ERROR', message: words, chain: words }
  }
}
