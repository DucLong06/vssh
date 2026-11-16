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
| `ui/`     | ratatui app: host list + details, terminal tabs and split panes (`panes.rs`), SFTP panes, forwards, snippets, popups, status line |
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

## Phase 5 — broadcast, split layouts, LazyVim-style UI

From `BRIEF-BROADCAST.md`, on branch `feat/broadcast-lazyvim`.

### B1 — broadcast core
- `Space` marks hosts; `Enter` with marks opens a session on each. `T` asks
  for a tag or group and connects every host that has it (exact match).
- Broadcast (`Alt-b`, `leader b`): what is typed or pasted in the focused
  session goes to every live session, encoded per session (a pane inside vim
  gets application-mode arrow keys, the others normal ones). All or nothing;
  there is no per-pane switch.
- Unit tests: key and paste routing with broadcast on and off, dead and
  non-terminal tabs skipped, per-session encoding.

### B2 — split layouts
- A pane is a session tab shown next to others; there is no separate pane
  tree. Layouts: single, 2 columns, 2x2 grid, 3 columns (`leader L` cycles,
  `leader w` then `s` `v` `g` `t` picks one). A sliding window decides which
  sessions are on screen, so a group opened together is shown together.
- Focus: `Alt-h/j/k/l` while several panes are shown, `leader` + arrow, or
  `leader w` + `h/j/k/l`. Close: `leader d` or `leader x`.
- Each pane has its own vt100 parser, PTY size and 5000 lines of scrollback.
- Unit tests: geometry tiles the area without gaps or overlap, focus moves,
  window sliding.

### B3 — bilingual installer and language setting
- `install.sh`: from a checkout, from a release binary, else `cargo install`.
  Messages in English or Vietnamese: `--lang`, `VSSH_LANG`, `LC_ALL`,
  `LC_MESSAGES`, `LANG`, first one set wins, `vi*` is Vietnamese.
- TUI: `lang = "vi"` in config.toml, `VSSH_LANG` or `--lang`. Translated: key
  hints, leader menu, help screen, status-line words. Forms, confirmations and
  error messages stay English (the strings are not centralised).
- Shell test of the installer's language selection, run by `cargo test`.

### B4 — LazyVim-style UI
- Status line like lualine: mode badge (NORMAL blue, SEARCH yellow, TERMINAL
  green, BROADCAST orange, SFTP teal, LEADER magenta; TokyoNight palette),
  context, `host : pane n/m` in the middle, tags, group and latency on the right.
- Key hints on their own line, grouped; keys that do nothing now are faded.
- Which-key popup after the leader, with a window submenu.
- `Ctrl-d` / `Ctrl-u` half-screen in the host list, `Q` quits.

### B5 — parity work
- Session logging: `leader l`, or `log_sessions = true`. Raw output to
  `~/.local/share/vssh/logs/<alias>-<YYYYMMDD-HHMMSS>.log`, mode 0600,
  starting with what is on screen. Nothing is masked.
- Login scripts: `[exec]` in config.toml (typed into the shell after login)
  and `RemoteCommand` from the ssh config (run instead of the shell).
- `leader s` types a snippet into the session (into all of them while
  broadcasting).

### Keys that changed in this phase

| Key | Before | Now |
|-----|--------|-----|
| `leader h` | host list | nothing; `leader q` or `leader 1` is the host list |
| `leader k` / `j` | scroll back / forward | nothing; `leader PgUp` / `PgDn` or `leader Ctrl-u` / `Ctrl-d` |
| `leader l` | nothing | toggle session log |
| `Alt-b` | sent to the shell (readline: back one word) | toggle broadcast |
| `Alt-h/j/k/l` | sent to the shell | pane focus, only while several panes are shown |
| `Ctrl-d` in the host list | acted as `d` (delete, with confirmation) | half a screen down |

The brief asks for pane focus on `h/j/k/l` and for `leader l` to toggle the
log. Both cannot sit on `leader l`, so focus moved to `Alt-h/j/k/l` and the
window menu.

### Feature audit against Tabby (Terminus) / Termius

| Feature | vssh |
|---------|------|
| Tabs, SFTP dual pane, forwards L/R/D, vault, tags, quick connect, known_hosts | yes (earlier phases) |
| Snippets | yes; typed into the active session with `leader s` since this phase (before, they only ran through a separate exec connection) |
| Split panes with layout presets | yes (B2) |
| Broadcast input to all sessions | yes (B1) |
| Session logging | yes (B5) |
| Startup snippet / login script | yes (B5) |

### Not planned

| Feature | Why |
|---------|-----|
| Zmodem (rz/sz) | Needs the terminal layer to intercept and speak the transfer protocol in-band; SFTP in the next tab already moves files, with progress and overwrite checks. |
| Serial terminal | Hardware and platform dependent (device nodes, line settings, permissions) and cannot be exercised in CI. The stub stays so such hosts fail with a clear message. |
| X11 forwarding | Useless without a local X server, and a TUI user is usually on a machine without one or already over SSH. Port forwards cover the TCP cases. |
| Web app | vssh is one static terminal binary; a web front end is a different product with its own authentication and hosting problems. |
| Jump hosts (`ProxyJump`, `ProxyCommand`) | Not honoured today: such hosts are refused with a message rather than connected directly and wrongly. Doing it properly means tunnelling the second SSH connection through a `direct-tcpip` channel of the first and verifying both host keys; that is a feature of its own. |
| Notification on completion | A terminal program cannot raise desktop notifications portably; the terminal bell already reaches the emulator the user runs. |
| Mouse | Keyboard-first by design; mouse reports would also have to be forwarded to remote programs correctly, which the emulator does not do. |
| Per-pane broadcast switch | All or nothing for now, as the brief allows. |
| Pane resize (`<` / `>`) | Marked optional in the brief; panes always split evenly. |

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
| Mouse support | None (see Not planned) |
| Vault entries on host rename/delete | Not migrated or removed; entries are keyed by alias |
| Terminal emulation | vt100 crate: colours, alternate screen, scrollback, bracketed paste, device-status replies; no mouse reporting, no OSC colour queries |

## Verification

- `cargo build --release`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.
- End to end against a throwaway local `sshd` on a high port with a temporary
  `HOME`, so the real `~/.ssh` is never touched: shell session, exec, SFTP
  transfer, local forward.
- Startup time and binary size measured on the release binary and reported in `RESULT.md`.
