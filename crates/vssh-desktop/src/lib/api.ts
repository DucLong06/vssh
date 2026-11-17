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

/** An entry of a directory, here or on a host. */
export type Entry = {
  name: string
  is_dir: boolean
  /** Bytes. */
  size: number
  /** Last change, Unix seconds; 0 = not known. */
  mtime: number
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

/** What `pty-state:<id>` says about a session's shell. */
export type PtyState =
  | { state: 'up' }
  /** The command in the foreground; `null` at the prompt. */
  | { state: 'busy'; command: string | null }
  | { state: 'closed'; error: ConnError | null }

export const api = {
  listHosts: () => invoke<Host[]>('list_hosts'),
  /** `passive`: the dashboard asking on its own; `null` where it may not. */
  getHostStats: (alias: string, passive = false) => invoke<Peek | null>('get_host_stats', { alias, passive }),
  /** TCP connect time in ms, `null` if down; rejects for a host behind a jump host. */
  pingHost: (alias: string) => invoke<number | null>('ping_host', { alias }),
  /** Resolves to the id of a new session, which connects when it is started. */
  sessionOpen: (alias: string) => invoke<number>('session_open', { alias }),
  /** `[user@]host[:port]`; resolves to the id of a new session. */
  quickConnect: (host: string) => invoke<number>('quick_connect', { host }),
  /**
   * Connects the session and runs its shell on a PTY of this size; again,
   * to reconnect. From then on `pty-data:<id>` carries its output as bytes
   * and `pty-state:<id>` a `PtyState`: listen before calling this.
   */
  sessionStart: (id: number, cols: number, rows: number) => invoke<void>('session_start', { id, cols, rows }),
  sessionWrite: (id: number, data: string) => invoke<void>('session_write', { id, data }),
  sessionResize: (id: number, cols: number, rows: number) => invoke<void>('session_resize', { id, cols, rows }),
  sessionClose: (id: number) => invoke<void>('session_close', { id }),
  /**
   * A person's yes to the host key that stopped the session (a `host_key`
   * failure names it), good for its next start; `forever` puts it on record.
   */
  sessionTrust: (id: number, host: string, fingerprint: string, forever: boolean) =>
    invoke<void>('session_trust', { id, host, fingerprint, forever }),
  /** The clipboard plugin's command: `navigator.clipboard` wants a user gesture, which an OSC 52 from a host is not. */
  copy: (text: string) => invoke<void>('plugin:clipboard-manager|write_text', { text }),
  /** A directory of this machine, directories first; no path is the user's home. `path` comes back in full. */
  localList: (path: string) => invoke<{ path: string; entries: Entry[] }>('local_list', { path }),
  localMkdir: (path: string) => invoke<void>('local_mkdir', { path }),
  /** A file, or a directory that is empty. */
  localRm: (path: string, isDir: boolean) => invoke<void>('local_rm', { path, isDir }),
  localRename: (from: string, to: string) => invoke<void>('local_rename', { from, to }),
  /** Where a path on the host leads, in full; no path is the directory the server starts in. */
  sftpRealpath: (host: string, path: string) => invoke<string>('sftp_realpath', { host, path }),
  sftpList: (host: string, path: string) => invoke<Entry[]>('sftp_list', { host, path }),
  sftpMkdir: (host: string, path: string) => invoke<void>('sftp_mkdir', { host, path }),
  sftpRm: (host: string, path: string, isDir: boolean) => invoke<void>('sftp_rm', { host, path, isDir }),
  sftpRename: (host: string, from: string, to: string) => invoke<void>('sftp_rename', { host, from, to }),
  /**
   * One file, to the full path given for it. Resolves when it is over;
   * `sftp-progress:<id>` carries `{ done, total }` meanwhile: listen first.
   */
  sftpUpload: (host: string, local: string, remote: string, id: string) =>
    invoke<void>('sftp_upload', { host, local, remote, id }),
  sftpDownload: (host: string, remote: string, local: string, id: string) =>
    invoke<void>('sftp_download', { host, remote, local, id }),
  /** Stops a transfer, which then rejects; what was written so far stays. */
  sftpCancel: (id: string) => invoke<boolean>('sftp_cancel', { id }),
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
