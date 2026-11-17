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

/**
 * The host form: what a host's own block of the config says, and its
 * metadata. Text left empty is taken out of the host.
 */
export type HostForm = {
  alias: string
  hostname: string
  user: string
  port: number | null
  identity_file: string
  /** The alias of the host it is reached through. */
  proxy_jump: string
  grp: string
  /** Space separated. */
  tags: string
  notes: string
  pinned: boolean
  /** Only ever sent: for the unlocked vault. Empty takes the stored one out; left out, it stays. */
  password?: string
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

/** `host` empty: for every host. */
export type Snippet = { id: number; host: string; name: string; command: string }

/** One host's answer in a batch: what the command printed, or why it could not be run. */
export type Answered = { id: number; host: string; output: string | null; error: ConnError | null }

/**
 * A port forward of a host: a line of its block in the ssh config. `L` and
 * `D` listen on `port` of this machine, `R` on the host's; `host:hport` is
 * where it leads (nothing for `D`). `value` is the line itself, the
 * forward's name for the commands.
 */
export type Forward = {
  kind: 'L' | 'R' | 'D'
  bind: string
  port: number
  host: string
  hport: number
  value: string
  state: 'off' | 'starting' | 'on'
}

/** What `forward-state` carries: a forward listens (`on`), or has ended by itself (`off`, with why). */
export type ForwardState = Pick<Forward, 'kind' | 'value'> & { host: string; state: 'on' | 'off'; error: ConnError | null }

export type ErrorClass =
  | 'dns'
  | 'timeout'
  | 'refused'
  | 'unreachable'
  | 'auth'
  | 'host_key'
  | 'port_in_use'
  | 'jump'
  | 'other'

export type Fail =
  | { kind: 'dns'; host: string }
  | { kind: 'timeout'; addr: string; secs: number }
  | { kind: 'refused'; addr: string }
  | { kind: 'unreachable'; addr: string }
  | { kind: 'auth'; user: string; host: string; keys: number; password: boolean }
  | { kind: 'host_key'; host: string; fingerprint: string; line: number; file: string }
  | { kind: 'port_in_use'; addr: string }
  | { kind: 'jump'; hop: string; cause: Fail | null }

/** What a command rejects with: a failure in no language yet. */
export type ConnError = { class: ErrorClass; fail: Fail | null; chain: string[] }

/** A failure put into words. */
export type Shown = { class: ErrorClass; label: string; message: string; chain: string }

/** `none`: there is no vault file yet. Unlocked, it stays so until the app is closed. */
export type VaultState = 'none' | 'locked' | 'unlocked'

/**
 * What `key-setup-step` carries. `backup` (of sshd_config) comes before
 * `install`; `restore` only after a step failed once the host was changed.
 */
export type KeyStep = {
  host: string
  step: 'generate' | 'backup' | 'install' | 'verify' | 'disable' | 'restore'
  state: 'running' | 'done' | 'failed'
}

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
  /** Those for every host first (`host` empty), then by host and name. */
  listSnippets: () => invoke<Snippet[]>('list_snippets'),
  /** Rejects for a snippet without a name or a command, or for a host that is not in the config. */
  snippetsCreate: (name: string, command: string, host: string) =>
    invoke<Snippet>('snippets_create', { name, command, host }),
  snippetsUpdate: (id: number, name: string, command: string, host: string) =>
    invoke<Snippet>('snippets_update', { id, name, command, host }),
  snippetsDelete: (id: number) => invoke<void>('snippets_delete', { id }),
  /**
   * One command on every host named, `{{name}}` in it replaced by
   * `params.name`. Resolves at once to the batch's id; each host then answers
   * with an `Answered` on `batch-progress`, maybe before the id is here:
   * listen first.
   */
  snippetRun: (hosts: string[], command: string, params: Record<string, string>) =>
    invoke<number>('snippet_run', { hosts, command, params }),
  forwardsList: (host: string) => invoke<Forward[]>('forwards_list', { host }),
  /** Into the host's block, not started. Rejects with `port_in_use` if the host has a forward on that port. */
  forwardAdd: (host: string, kind: Forward['kind'], localPort: number, remote: string | null) =>
    invoke<Forward>('forward_add', { host, kind, localPort, remote }),
  /** Starts a forward, or stops it if it runs: true if it is now starting. `forward-state` says when it listens. */
  forwardToggle: (host: string, kind: Forward['kind'], value: string) =>
    invoke<boolean>('forward_toggle', { host, kind, value }),
  /** Stops it if it runs, and takes it out of the config. */
  forwardKill: (host: string, kind: Forward['kind'], value: string) =>
    invoke<void>('forward_kill', { host, kind, value }),
  /** A host's form as it stands. */
  hostsForm: (alias: string) => invoke<HostForm>('hosts_form', { alias }),
  /** Rejects for an alias that is taken or is not one word, and for a password while the vault is locked. */
  hostsAdd: (form: HostForm) => invoke<Host>('hosts_add', { form }),
  /** Saves the form over the host `alias`; another alias in the form renames it. */
  hostsUpdate: (alias: string, form: HostForm) => invoke<Host>('hosts_update', { alias, form }),
  /** With its metadata, its snippets, its forwards and its password in the unlocked vault. */
  hostsDelete: (alias: string) => invoke<void>('hosts_delete', { alias }),
  /** The hosts of another ssh config (`null`: `~/.ssh/config`) that the app does not have yet. */
  hostsImportPreview: (path: string | null) => invoke<Host[]>('hosts_import_preview', { path }),
  /** Takes those of them over that `only` names, and resolves to the aliases added. */
  hostsImport: (path: string | null, only: string[]) =>
    invoke<string[]>('hosts_import_ssh_config', { path, only }),
  /**
   * A new key for the host, proven to open it, then its password login is
   * turned off. Resolves to the private key's path; `key-setup-step` carries
   * a `KeyStep` for each step meanwhile: listen first.
   */
  sshKeySetup: (host: string) => invoke<string>('ssh_key_setup', { host }),
  vaultState: () => invoke<VaultState>('vault_state'),
  /** Unlocks the vault; where there is none, makes it with this passphrase. */
  vaultUnlock: (passphrase: string) => invoke<void>('vault_unlock', { passphrase }),
  /** The hosts the unlocked vault has a password for; none while it is locked. */
  vaultHosts: () => invoke<string[]>('vault_hosts'),
  vaultPassChange: (old: string, next: string) => invoke<void>('vault_pass_change', { old, new: next }),
  /** A sealed copy of the unlocked vault, in a file that must not exist yet. */
  vaultExport: (path: string) => invoke<void>('vault_export', { path }),
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
