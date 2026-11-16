# vssh — feature plan

A terminal SSH connection manager in Rust: ctty's feature set, Termius-style host
management, k9s-style layout, vim keys. One binary, one async runtime, no GC.

## Principles

- `~/.ssh/config` stays the source of truth for hosts. vssh edits it losslessly
  (comments, unknown options, `Match` blocks and ordering survive) and always
  leaves a backup before the first write. Everything OpenSSH has no field for
  (group, tags, notes, pin, usage stats, snippets) lives in SQLite.
- Secrets live only in the vault file and in zeroized memory. Nothing secret is
  logged, put in error messages, or written to SQLite.
- The UI thread never blocks on the network. Every connection, transfer and
  forward is a tokio task that reports back over one event channel. The screen
  is redrawn on events only, so idle CPU is zero.
- Host keys are verified against `~/.ssh/known_hosts` (unknown = learn on first
  use, changed = refuse).

## Layout

Single crate, one directory per concern:

| Module    | Contents |
|-----------|----------|
| `store/`  | `ssh_config.rs` (lossless parser/writer, pattern matching, forward specs), `db.rs` (SQLite metadata + snippets) |
| `vault/`  | Encrypted password store (Argon2id + XChaCha20-Poly1305) |
| `ssh/`    | russh transport: connect, known_hosts, auth, shell sessions, exec, `forward.rs` (local / remote / SOCKS5 dynamic) |
| `sftp/`   | SFTP worker task: listing, upload/download with progress, mkdir/rename/delete |
| `ui/`     | ratatui app: host list + details, terminal tabs, SFTP panes, forwards, snippets, popups |
| `config.rs` | `~/.config/vssh/config.toml` (theme, keybindings) |
| `proto.rs`  | Telnet/Serial stub |

Files on disk:

- `~/.ssh/config`, `~/.ssh/known_hosts` — shared with OpenSSH
- `~/.config/vssh/config.toml` — theme and keys (the brief says `config.tom`; read as a typo)
- `~/.local/share/vssh/vssh.db` — SQLite metadata
- `~/.local/share/vssh/vault` — encrypted passwords, mode 0600

## Milestones

### M1 — skeleton + TUI shell
- Cargo project, release profile (opt-level 3, LTO, strip, one codegen unit, panic=abort).
- Config file loading with defaults (theme colours, hosts-view keys, leader key).
- Event loop: one input thread feeding a tokio channel; draw on event.
- k9s-style frame: tab bar, host list left, details right, key-hint footer, `?` help.
- Terminal is always restored, including on panic.

### M2 — host store + ssh config
- Lossless `~/.ssh/config` parser/writer; `Host` pattern matching (`*`, `?`, `!`)
  so `Host *` defaults apply the way OpenSSH applies them.
- Atomic writes (temp file + rename, 0600) and a one-time `config.vssh.bak`.
- SQLite metadata: group, tags, notes, pinned, last-connected, connection count.
  Hosts in the ssh config are imported (metadata rows created) on first run and
  kept in sync on every start.
- TUI: add / edit / delete / duplicate host, pin, group + tag chips with
  stable per-name colours, fuzzy search (`/`), sort pinned first.
- TCP reachability check with latency, shown as a status dot.
- Unit tests: parser round-trip, option lookup, pattern matching, edits, forward specs.

### M3 — SSH connect + tabs
- russh client: known_hosts verification, auth order agent → identity files →
  vault password → prompted password.
- Interactive shell on a PTY rendered through a vt100 emulator into a ratatui
  pane; resize is forwarded; scrollback.
- Any number of concurrent sessions, one tab each. `Tab` cycles tabs from the
  host list; inside a session every key goes to the remote, so tab control is
  behind a leader key (`Ctrl-a` by default).
- Usage stats (last connected, count) recorded on connect.

### M4 — SFTP, forwarding, vault, snippets
- SFTP: dual pane (local left, remote right), copy across panes in either
  direction with a progress gauge, mkdir, rename, delete.
- Forwards: local, remote and dynamic (SOCKS5), stored as standard
  `LocalForward` / `RemoteForward` / `DynamicForward` lines in the ssh config so
  plain `ssh` honours them too; start/stop each one from the TUI.
- Vault: master passphrase → Argon2id → XChaCha20-Poly1305, fresh nonce per
  save, keys and plaintext zeroized. Set/remove a host password from the TUI.
- Snippets: global or per-host commands in SQLite; run against the selected
  host or all marked hosts, output collected per host.
- Unit tests: vault round-trip, wrong passphrase, tampering.
- README with build/run instructions and the key cheat sheet.

## Scoped out (stubbed or deferred)

| Item | State |
|------|-------|
| Serial / Telnet | Stub module only: such hosts are recognised and report "not implemented" |
| `ProxyJump` / `ProxyCommand` | Preserved in the config, not honoured when connecting |
| `Include` in ssh config | Line preserved, included files are not read |
| Passphrase-protected key files | Not prompted for; use the agent |
| keyboard-interactive / 2FA auth | Not supported |
| Recursive directory transfer in SFTP | Files only |
| Remapping keys in sub-views | Only the leader and host-list keys are configurable |
| Mouse support | None |
| Vault entries on host rename/delete | Not migrated or removed; entries are keyed by alias |
| Terminal emulation | vt100 crate: colours, alternate screen, scrollback, bracketed paste, device-status replies; no mouse reporting, no OSC colour queries |

## Verification

- `cargo build --release`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.
- End to end against a throwaway local `sshd` on a high port with a temporary
  `HOME`, so the real `~/.ssh` is never touched: shell session, exec, SFTP
  transfer, local forward.
- Startup time and binary size measured on the release binary and reported in `RESULT.md`.
