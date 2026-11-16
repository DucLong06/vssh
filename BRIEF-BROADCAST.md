# BRIEF 5 — Broadcast multi-SSH + Terminus parity + LazyVim UI overhaul

Project: vssh (Ratatui TUI SSH manager, Rust). Continue from existing codebase — read docs/PLAN.md, src/, RESULT.md first. Language: code/comments English, UI strings English or as specified below.

## Goals (in priority order)

### 1. Multi-session broadcast (top priority)
- Allow connecting to MULTIPLE hosts at once and viewing them side-by-side:
  - Multi-select hosts in the host list (e.g. Space to toggle, then Enter to connect all selected), or "connect all in tag" (e.g. `T` on a tag connects every host with that tag).
  - Session view layouts (Termimus-style presets): 1 (single), 2 (side-by-side), 2x2 grid, 3-column; key `Ctrl-a` leader then `L` to cycle layouts; panes focusable with `h/j/k/l` (lazyvim window nav), resize with `<`/`>` optional.
- Broadcast mode: toggle `Alt-b` (also `Ctrl-a b`). When ON, keystrokes typed in the focused pane are sent to ALL connected sessions (not just focused). Show a clear BROADCAST indicator in the status bar (Termimus SYNC style). Per-pane enable is optional (keep simple: all-or-nothing first).
- Each pane is a vt100 session with its own scrollback. Closing pane: `x` (leader `d` closes current session like vim buffer delete).

### 2. install.sh bilingual (EN/VI)
- The one-line installer (install.sh) must print its messages in English OR Vietnamese: auto-detect from `$LANG`/`$LC_ALL` (vi* → Vietnamese), override with `--lang en|vi` and `VSSH_LANG` env var.
- Also add a `--lang`/`lang` setting for the TUI itself (status bar, help texts) if UI strings are centralized; keep default = English, config `lang = "vi"` in config.toml.

### 3. Terminus (Tabby) / Termimus feature parity — audit + add
Audit vssh against Terminus/Termimus features and implement the feasible ones for a TUI; document the rest in docs/PLAN.md as "not planned":
- [x] has: tabs, SFTP dual-pane, port forwards L/R/D, vault, snippets (with active-tab injection), tags, quick-connect, known_hosts.
- ADD: session logging (toggle per session with `Ctrl-a l` → writes pane scrollback/output to `~/.local/share/vssh/logs/<alias>-<date>.log`, warn + mask nothing, config default off), login scripts (host field `exec` command run on connect; per-host in config.toml + ssh config `RemoteCommand` respect), broadcast (goal 1), split layouts (goal 1).
- NOT planned (document why): Zmodem, serial terminal, Zmodem X11 forwarding, web app, jump host (only if ProxyJump in ssh config is already honored by russh path — if not, document as not planned), notification on completion, mouse support beyond existing.

### 4. LazyVim-style UI + keybindings (user loves LazyVim)
- Status bar bottom like lualine: left = mode indicator with colored background (NORMAL = TokyoNight blue, BROADCAST = orange/red, SFTP = teal), center = host:pane, right = latency + tag + git-style branch-ish info.
- Keyhints footer grouped by context, fading inactive (dim) hints — lazyvim noice style.
- Which-key popup after `Ctrl-a` (exists) — extend with new keys: `b` broadcast, `L` layout cycle, `l` log toggle, `d` close pane, `w` window/pane menu.
- Keybindings follow LazyVim (keep existing vim ones): `h/j/k/l` pane nav, `Ctrl-d`/`Ctrl-u` half scroll in host list, `gg`/`G` top/bottom (or existing), `/` search, `?` help, `Esc` clears popup/search, `<Tab>`/`<S-Tab>` cycle tabs, `Space` multi-select, `q` quit pane view (back to host list), `Q` quit app.
- All new keys documented in README cheat sheet (both EN and VI README sections).

## Rules
- NO fabricated output: run cargo fmt, build, test, clippy for real and paste actual results into RESULT.md.
- Commit after each milestone (M1 broadcast core, M2 layouts, M3 install lang, M4 lazyvim ui, M5 parity + docs). Author: Hoàng Đức Long <hoangduclongg@gmail.com>.
- Work on branch `feat/broadcast-lazyvim` from current master. Do NOT push, do NOT touch main.
- Update docs/PLAN.md + README (EN + VI) + RESULT.md as you go.
- Tests: unit tests for broadcast routing (key → all sessions), layout geometry, installer language detection (shell test), login script exec.