# Task: vssh UI restyle (k9s/lazygit/Termius inspired) + README screenshots + quick-connect

The user finds the current TUI flat/monotonous. Restyle it, keeping all existing keybindings and functionality.

## Design direction (references: k9s, lazygit, Termius, Charm/lipgloss aesthetics)
1. **Host list rows — two-tier**: alias bold/bright on line 1; second line dim gray: `user@host  ·  group  [tag] [tag]`; tag badges in per-tag colors (stable color per tag name via hash).
2. **Right detail panel** (k9s style): when no session is open, show selected host details — address, port, user, key path, group/tags/notes, last connected, connection count, reachability. Border color follows focus.
3. **Status dot column**: green (reachable, latency), red (timeout), gray (unknown/unchecked); latency in ms shown dim.
4. **Borders & focus**: thick/accent-colored border on focused panel, muted otherwise; consistent 1-cell padding.
5. **Header bar**: `vssh` name, active filter/search shown as breadcrumb; bottom status bar with key hints (like lazygit), updating per focus mode.
6. Keep the vim keys and the existing config.toml theming; add new theme entries with sane defaults so existing configs still work (missing keys fall back to defaults).

## Quick-connect (new feature, small)
- From host list, press `c` (or another free key) to connect WITHOUT saving: form asks `user@host[:port]`, optional password (used in-session only, never stored), connects into a tab.
- Do not write anything to ~/.ssh/config for quick-connect.

## README screenshots
- Update README to embed real screenshots/GIF of the NEW UI (both English and Vietnamese sections — same media).
- Use [vhs](https://github.com/charmbracelet/vhs) if installable (tape -> GIF/MP4), or fall back to terminal screenshots saved under `docs/img/`. If vhs/network is unavailable, take real terminal captures via a script (e.g., run the TUI in tmux, capture pane) — the media must show the real app, never mockups.
- Add a `docs/img/` directory; reference from README with relative paths.

## Verification
- cargo fmt --check, cargo build --release, cargo test, cargo clippy -- -D warnings — all real runs, outputs appended to RESULT.md.
- Install the rebuilt binary to ~/.local/bin/vssh so the user can try it immediately.
- Commit in small logical commits; do not rewrite history; do not fabricate output. If a design item turns out too large, implement the rest and note it.
