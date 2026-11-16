# Project: vssh — Rust TUI SSH connection manager

## Goal
Build a clean, fast, full-featured terminal SSH manager in Rust, inspired by ctty (features) and Termius (host management UX), with k9s/lazydocker-style UI and vim keybindings. Must be materially faster than the Go implementations (startup < 50ms, no GC pauses).

## Feature requirements (parity or better vs ctty/Termius)
1. Host management: read/write `~/.ssh/config` (standard OpenSSH format, compatible with existing config), plus a local SQLite store for metadata (groups, tags, notes, last-connected, connection count, pin state). Import hosts from ssh config on first run. Add/edit/delete/duplicate hosts from the TUI.
2. Connect: SSH sessions via russh (pure Rust SSH implementation), auth with key files, agent, or encrypted password vault. Multiple concurrent sessions in tabs (Tab key cycles, leader-style).
3. SFTP dual-pane file browser (local <-> remote) with upload/download, mkdir, rename, delete, progress display.
4. Port forwarding: local/remote/dynamic forwards, startable/stoppable per host from the TUI.
5. Local credential vault: file-encrypted (age or ChaCha20-Poly1305) store for passwords, unlocked by master passphrase. NEVER log secrets.
6. Snippets: per-host or global command snippets, runnable against selected host(s).
7. Serial/Telnet support can be a stub module — SSH + SFTP are the core.
8. Search/fuzzy filter hosts (j/k or arrow keys navigate, `/` to search, Enter connect).
9. Host groups/tags with colors, status indicators (reachable check via TCP ping).
10. Config: `~/.config/vssh/config.tom` for theme and keybindings.

## Non-functional
- Rust edition 2021+, async with tokio, TUI with ratatui + crossterm.
- Crates: russh, russh-keys (or russh-sftp), tokio, ratatui, crossterm, fuzzy-matcher, rusqlite (bundled), zeroize, age or chacha20poly1305.
- Startup < 50ms, binary size target < 8MB (strip + opt-level = 3, LTO).
- Modular crate layout: `ssh/` (transport), `store/` (config + sqlite), `vault/`, `ui/` (ratatui screens), `sftp/`.
- Vim keybindings everywhere; k9s-style layout: left panel host list, right panel details/session.
- Dark theme, high contrast accent colors, clean borders.

## Deliverables (do them in order, commit after each)
1. `docs/PLAN.md` — detailed feature plan with milestones M1-M4 (M1 skeleton+TUI shell, M2 host store + ssh config parse, M3 SSH connect + tabs, M4 SFTP + forwarding + vault + snippets). Write this FIRST and commit it.
2. Working Rust project that `cargo build --release` cleanly and `cargo clippy -- -D warnings` passes.
3. Unit tests for ssh-config parser and vault crypto (round-trip). `cargo test` green.
4. A short `README.md` with build/run instructions and keybinding cheat sheet.

## Rules
- Commit in small logical steps with clear messages.
- Do not fabricate test output; run cargo build/test/clippy for real and paste real results into the final summary.
- Keep code clean: clippy-clean, rustfmt formatted.
- If a crate API is uncertain, check the crate docs in ~/.cargo/registry rather than guessing.
- Scope control: if a feature (e.g. serial) is too large, stub it and note it in PLAN.md.
