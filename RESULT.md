# vssh — result

Everything below was run on this machine on 2026-11-16 (Linux 6.14, rustc 1.95.0,
cargo 1.95.0). Command output is pasted as produced; nothing is reconstructed.
The code checked is commit `cea957a` (the last code/docs commit before this file).

## What was built

A single-binary Rust TUI SSH manager, 4,054 lines of Rust in one crate with a
module directory per concern, as laid out in `docs/PLAN.md`.

| Area | State |
|------|-------|
| Host management | Lossless `~/.ssh/config` parser/writer (comments, unknown options, `Match` blocks, ordering preserved; OpenSSH first-match resolution incl. `*`, `?`, `!` patterns; atomic 0600 writes; one-time `config.vssh.bak`). Add / edit / duplicate / delete from the TUI. |
| Metadata | SQLite (`rusqlite`, bundled): group, tags, notes, pin, last-connected, connection count, snippets. Hosts are imported from the ssh config on every start. |
| SSH | `russh` 0.65 (pure Rust). `known_hosts` check (unknown = learn, changed = refuse). Auth: agent → key files → vault password → prompted password. Sessions run in tabs through a `vt100` emulator; resize, scrollback, bracketed paste, alternate screen. |
| SFTP | Dual pane (local / remote): navigate, upload, download with progress gauge, mkdir, rename, delete, overwrite confirmation. |
| Forwarding | Local, remote and dynamic (SOCKS5), stored as `LocalForward` / `RemoteForward` / `DynamicForward` in the ssh config, started and stopped per host. |
| Vault | Argon2id (19 MiB, t=2) + XChaCha20-Poly1305, fresh nonce per save, 0600 file, zeroized keys and plaintext. |
| Snippets | Global or per-host, run on the selected host or all marked hosts, output per host. |
| Search / status | Fuzzy filter (`fuzzy-matcher`), group/tag colours, TCP reachability with latency. |
| Config | `~/.config/vssh/config.toml`: theme colours, leader and host-list keys. |
| Serial / Telnet | Stub only (`src/proto.rs`): such hosts are refused with "not implemented". |

Deliverables: `docs/PLAN.md` (written and committed first), the project,
unit tests, `README.md` with build/run instructions and the key cheat sheet.

## Build, test, lint: real output

### `cargo fmt --check`

```
exit code: 0
```

(No output means no formatting differences.)

### `cargo build --release` (after `cargo clean --release`)

```
   Compiling libc v0.2.191
   Compiling cfg-if v1.0.5
   Compiling subtle v2.6.1
   ... (223 more "Compiling" lines elided) ...
   Compiling russh v0.65.1
   Compiling rusqlite v0.39.0
   Compiling vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `release` profile [optimized] target(s) in 2m 31s
exit code: 0
```

229 crates compiled, 0 warnings.

```
-rwxrwxr-x 2 longhd longhd 6147704 Nov 16 15:59 target/release/vssh
vssh 0.1.0
```

Binary size: 6,147,704 bytes (5.86 MiB), under the 8 MB target. Profile:
`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.

### `cargo test`

```
   Compiling vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.33s
     Running unittests src/main.rs (target/debug/deps/vssh-a4fc87fab061ca8c)

running 31 tests
test config::tests::partial_config_keeps_defaults ... ok
test store::ssh_config::tests::first_host_in_empty_file ... ok
test config::tests::rejects_typos_and_bad_colours ... ok
test ssh::forward::tests::socks5_connect_requests ... ok
test store::ssh_config::tests::parses_separators_quotes_and_case ... ok
test ssh::tests::target_resolution ... ok
test store::ssh_config::tests::round_trip_is_lossless ... ok
test store::ssh_config::tests::multi_valued_add_remove ... ok
test store::db::tests::metadata_and_snippets ... ok
test ui::draw::tests::formats_sizes_and_ages ... ok
test store::ssh_config::tests::forwards_parse_and_format ... ok
test store::ssh_config::tests::lists_only_concrete_hosts ... ok
test ui::term::tests::answers_terminal_queries ... ok
test ui::files::tests::remote_paths ... ok
test ui::term::tests::encodes_keys_like_xterm ... ok
test ui::term::tests::renders_text_colours_and_wide_chars ... ok
test ui::tests::clean_strips_escape_sequences ... ok
test ssh::forward::tests::socks5_rejects_what_it_cannot_do ... ok
test store::ssh_config::tests::set_replaces_adds_and_removes ... ok
test store::ssh_config::tests::host_lifecycle ... ok
test store::ssh_config::tests::glob_patterns ... ok
test ui::draw::tests::chips_are_stable_and_popups_fit ... ok
test store::ssh_config::tests::resolves_like_openssh ... ok
test ui::files::tests::local_pane_operations ... ok
test store::ssh_config::tests::save_backs_up_once_and_is_private ... ok
test vault::tests::set_takes_the_secret_out_of_the_caller ... ok
test vault::tests::file_is_private_opaque_and_renonced ... ok
test vault::tests::create_refuses_to_overwrite_or_use_empty_passphrase ... ok
test vault::tests::wrong_passphrase_is_rejected ... ok
test vault::tests::round_trip ... ok
test vault::tests::tampering_is_detected ... ok

test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.25s

exit code: 0
```

31 tests: ssh-config parser (11), vault crypto (6, including round trip, wrong
passphrase, tampering), SOCKS5 handshake (2), SQLite store (1), target
resolution (1), config (2), terminal key encoding / rendering / query replies
(3), SFTP pane logic (2), UI helpers (3).

### `cargo clippy -- -D warnings`

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.83s
exit code: 0
```

`cargo clippy --all-targets -- -D warnings` (tests included) is clean as well:

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
exit code: 0
```

## Startup time

Measured on the release binary with a small Python harness: allocate a real
120x40 pty, exec `vssh`, time until the full first frame (host list title and
footer) has been written, then send `q` and wait for exit code 0.

| Scenario | First frame (median) | min / max | Runs |
|----------|----------------------|-----------|------|
| 5 hosts | 2.96 ms | 2.04 / 6.11 ms | 50 |
| 1000 hosts | 16.84 ms | 13.07 / 36.23 ms | 50 |
| 1000 hosts, very first run (creates the SQLite db, imports 1000 rows) | 59.54 ms | n/a | 1 |

So the 50 ms target is met in normal use; the one-time first launch with a
1000-host config was 60 ms. No comparison against ctty or other Go tools was
run, so "faster than the Go implementations" is not measured, only the
absolute numbers above.

## End-to-end checks against real servers

Driven through the real TUI in tmux, with `HOME` pointed at a temp directory so
the real `~/.ssh` was never touched (its `config` and `known_hosts` mtimes were
checked before and after and are unchanged). Servers: a throwaway OpenSSH
`sshd` on 127.0.0.1:22222 (public key auth), and an Alpine container running
`sshd` on 127.0.0.1:22223 (password auth). Both were removed afterwards.

| Check | Result |
|-------|--------|
| First frame, host list, details panel | ok |
| Reachability check (`r`) | ok: live host shows `reachable (0 ms)`, dead port is flagged (colours not checked, captures were plain text) |
| Shell tab, key-file auth (ed25519) | ok: `sshd` logged `Accepted publickey ... ED25519` |
| Key-file auth (RSA 3072, no agent) | ok: `Accepted publickey ... RSA` |
| Agent auth (private key file removed, key only in agent) | ok |
| Password from the vault (container) | ok: `Accepted password for root`, no prompt |
| Keys refused, no vault → masked prompt → wrong password → prompt again → right password | ok |
| PTY size and resize forwarding | ok: `tput cols/lines` 120x30, then 100x26 after resizing |
| Several sessions, leader `Ctrl-a` + `Tab` / `h` / digit / `x` | ok |
| nvim (alternate screen, cursor keys), `less`, scrollback, bracketed paste | ok |
| Remote `exit` → tab marked closed | ok |
| Unknown host key learned into the temp `known_hosts` | ok |
| Changed host key | ok: refused with `HOST KEY MISMATCH` |
| Dead host | ok: `Connection refused` shown in the tab |
| SFTP upload 3 MB and 400 MB | ok: `cmp` identical; 400 MB took 1.8 s over loopback, gauge visible mid-transfer |
| SFTP download, mkdir, rename, delete, enter/leave directory | ok |
| SFTP overwrite | ok: asks first |
| Local forward `L 18080 127.0.0.1:18081` | ok: `curl` got the page |
| Remote forward `R 19090 127.0.0.1:18081` | ok |
| Dynamic forward `D 11080` | ok: `curl --socks5` and `--socks5-hostname`; closed target → curl exit 7 |
| Stop forwards | ok: listeners gone, their `sshd` sessions gone |
| Add / delete forward from the TUI | ok: written to / removed from the ssh config; bad syntax rejected |
| Vault create (mismatch rejected), store, lock, wrong passphrase, unlock | ok; vault file 0600, no plaintext in vault or db |
| Snippet add, run on one host, run on two marked hosts, edit validation, delete | ok: output and non-zero exit status shown, dead host reported per host |
| Host add (port validation), edit/rename, duplicate, pin, delete, fuzzy search | ok |
| ssh config after all edits | ok: comment and layout preserved, backup written once |
| Quit with an open tab (`q`, `Ctrl-c`) | ok: asks first, exit code 0 |

These runs were spread over several release builds made while fixing what they
found (below). The last pass (RSA auth, nvim, less, paste, quit, startup
timing) ran on a binary built from the same source as the final commit; the
earlier rows were not all repeated on that last binary.

### Bugs found by the live runs and fixed

1. New hosts were appended after a trailing `Host *`; since OpenSSH keeps the
   first value it sees, the wildcard defaults would have overridden the new
   host's own settings. Now inserted ahead of the first matching wildcard block (unit test added).
2. Closing a tab moved the view to whatever tab took its index, so following
   keystrokes could land in another live shell. Now returns to the host list.
3. A new search query left the cursor on the previously selected row instead of the best match.
4. The emulator did not answer terminal status queries; nvim waited about 1.1 s
   on exit. It now replies to DA1/DA2, device status and cursor position (0.07 s; unit test added).
5. A unit test for the SOCKS5 handshake hung (test helper never half-closed the stream).

## Deviations from the brief

- `config.tom` in the brief is read as a typo; the file is `~/.config/vssh/config.toml`.
- "Modular crate layout" is one crate with the directories `ssh/`, `store/`,
  `vault/`, `ui/`, `sftp/`, not a multi-crate workspace.
- `russh-keys` no longer needs to be a separate dependency; in russh 0.65 it is `russh::keys`.
- Crypto: ChaCha20-Poly1305 (the XChaCha variant, for safe random nonces) with Argon2id, not `age`.
- Extra crates beyond the brief's list: `vt100` (terminal emulation), `argon2`, `toml` + `serde`, `anyhow`.
- russh is built with the `ring` backend instead of the default `aws-lc-rs`.

## Not done / not verified

Not implemented (also listed in `README.md` and `docs/PLAN.md`):
`ProxyJump` / `ProxyCommand` (such hosts refuse to connect), `Include`,
passphrase-protected key files, keyboard-interactive / 2FA, recursive directory
transfer, mouse, serial/telnet (stub), remapping keys outside the host list.
Vault entries are not moved or removed when a host is renamed or deleted.

Not verified:
- macOS (written for Unix, only run on Linux).
- ECDSA keys, host certificates, servers that only offer keyboard-interactive.
- SFTP and forwards over a slow or lossy link; all tests were on loopback.
- A custom `config.toml` in the running app (parsing is unit-tested only).
- Long-running stability (hours-long sessions, reconnect behaviour: there is no auto-reconnect).
- No automated integration test exists for the SSH paths; the table above was a manual, scripted run and is not reproducible from `cargo test`.

## Commits

```
cea957a docs: README with build instructions and key cheat sheet, plan notes
e501bf5 feat(ui): answer terminal status queries in session tabs
0d752b6 fix(ui): tab closing, search selection and quit confirmation
9988787 fix(store): add new hosts ahead of a matching wildcard block
ba925cb feat: ssh transport, forwarding, sftp worker and the TUI
fc900ec feat: project skeleton, config, ssh config store, sqlite metadata, vault
020f59c docs: add feature plan with milestones M1-M4
```

`BRIEF-CI.md` appeared in the project directory during this work. It is not
mine, was not part of this task, and is left untracked and unread.
