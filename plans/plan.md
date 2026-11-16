---
title: vssh — Rust TUI SSH connection manager
status: in-progress
priority: P1
effort: large
branch: init/main
repo: git@github.com:DucLong06/vssh.git
tags: [rust, tui, ssh, open-source]
created: 2026-11-16
---

# vssh — Rust TUI SSH connection manager

Mục tiêu: SSH manager TUI bằng Rust theo phong cách k9s/lazygit + lazyvim (TokyoNight), đủ chức năng ngang ctty/Termius, nhanh hơn bản Go. Open-source MIT, repo github.com/DucLong06/vssh.

## Phases

### Phase 1 — Core implementation [x]
- [x] PLAN.md milestones M1-M4
- [x] ssh config parser/writer (lossless, atomic, 0600)
- [x] SQLite metadata (group, tags, notes, pin, stats, snippets)
- [x] SSH transport (russh): auth agent → key → vault password → prompt; known_hosts accept-new
- [x] Sessions trong tabs (vt100 emulator), SFTP dual-pane (`c` copy, progress), forwards L/R/D
- [x] Vault: Argon2id + XChaCha20-Poly1305, zeroize, 0600, tamper tests

### Phase 2 — CI + security tests + README song ngữ [x]
- [x] GitHub Actions: fmt, build, test, clippy, cargo audit
- [x] Security tests: vault tamper, secrets không lọt Debug/Display/log, parser edge cases, SQL parameterized
- [x] README EN + VI: quickstart, thêm server (TUI + ssh config), group/tag, cheat sheet

### Phase 3 — UI restyle + quick-connect [in-progress]
- [ ] Two-tier host rows, tag badge màu, status dot + latency, focus borders, header/bottom bar (đang chạy)
- [ ] Quick-connect: connect không lưu host (đang chạy)
- [ ] Screenshot/GIF thật vào docs/img/, nhúng README (đang chạy)
- [ ] Default theme TokyoNight Night (bảng màu Ghostty của Long), which-key popup kiểu lazyvim sau leader Ctrl-a

### Phase 4 — Verification + release [pending]
- [ ] Independent verification: fmt/build/test/clippy/audit chạy tay
- [ ] Security review (skill security-scan + Claude security-review), findings sửa trước PR
- [ ] Đóng gói: install.sh một dòng, GitHub Releases binary, update = chạy lại install
- [ ] Thêm vssh vào DucLong06/terminal-setup (PR riêng)
- [ ] Rewrite author các commit → Hoàng Đức Long <hoangduclongg@gmail.com>, push nhánh, init PR
- [ ] Branch protection main (gh api), báo Long merge

## Key paths
- Project: ~/projects/vssh (BRIEF.md, BRIEF-CI.md, BRIEF-UI.md, RESULT.md, docs/PLAN.md)
- Binary cài thử: ~/.local/bin/vssh
- Log Claude: ~/.hermes/cache/scratch/vssh-claude*.log
- Wiki: entities/vssh.md (~/wiki)

## Quy ước
- Commit: Hoàng Đức Long <hoangduclongg@gmail.com>. Không push thẳng main → PR.
- Theme: TokyoNight Night. Keybinding: vim (j/k/g/G), leader Ctrl-a.
- Không bịa output — mọi kết quả build/test phải chạy thật.
