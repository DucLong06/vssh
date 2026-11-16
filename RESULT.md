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


---

# Update: CI, security tests, bilingual README (BRIEF-CI.md)

Run on 2026-11-16 on the same machine, at commit `30741f0`. Output is pasted
as produced. This section supersedes the numbers above where they differ
(test count, binary size, commit list, and the note that `BRIEF-CI.md` was unread).

## What was added

| Item | Where |
|------|-------|
| GitHub Actions CI: job `build-test` (`cargo fmt --check`, `cargo build --release`, `cargo test`, `cargo clippy --all-targets -- -D warnings`) and job `audit` (`cargo audit`), both cached with `Swatinem/rust-cache@v2`; runs on every push and on PRs to `main`/`master` | `.github/workflows/ci.yml` |
| `cargo audit` configuration with one documented ignore | `.cargo/audit.toml` |
| Security integration tests (11) | `tests/security.rs` |
| More in-crate tests (38 total, was 31): host key policy, vault Debug/error leaks, malformed vault plaintext, line-break injection, pattern matcher, tag filter | `src/**` |
| Library target so `tests/` can import the modules | `src/lib.rs` |
| Clippy denies `print_stdout`, `print_stderr`, `dbg_macro` (nothing may print or log while the TUI owns the terminal) | `Cargo.toml` |
| `#tag` search filter (needed to document "filter by tag" truthfully) | `src/ui/mod.rs` |
| README rewritten: English then Vietnamese, mirrored | `README.md` |

The workflow has not run on GitHub: nothing was pushed. The README badge will
show no status until the first push. Locally the YAML parses and every command
in it was run, below.

## Security problems found while writing the tests, and fixed

1. **Host key type switch bypassed the known_hosts check.** A host on record
   with a key of one type (say ECDSA or RSA) was treated as *unknown* when the
   server presented another type, and the new key was learned silently, so a
   known host could be impersonated. Now a host with any recorded key must
   present a recorded key, and recorded key types are offered first during
   negotiation (as OpenSSH does). Checked live against an `sshd` with RSA and
   Ed25519 host keys: recorded-RSA-only host connects (verified via RSA,
   nothing added to `known_hosts`); a different recorded RSA key is refused
   with `HOST KEY MISMATCH ... line 1`; an unknown host is learned.
2. **SQLite advisory RUSTSEC-2026-0459** (`libsqlite3-sys` 0.37, found by
   `cargo audit`): fixed by upgrading `rusqlite` 0.39 → 0.43.
3. **Dead session closed on any key.** When a connection dropped, the next key
   closed the tab and the rest of a half-typed line ran as host-list commands
   (`e` opens the edit form, Enter saves). This renamed a host in the test
   config during the live run. A dead tab now ignores keys and closes only with `Ctrl-a x`.
4. **Line breaks in values written to the ssh config** could have added
   directives. Not reachable from the TUI (paste and input already strip
   them), now also impossible at the writer.
5. **Host pattern matching was exponential** for patterns with many `*`; a
   crafted `Host` line could hang startup. Now O(pattern × text).

## Not fixed: RUSTSEC-2023-0071 (rsa, "Marvin" timing side channel)

`cargo audit` without the ignore entry:

```
    Fetching advisory database from `https://github.com/RustSec/advisory-db.git`
      Loaded 1509 security advisories (from /home/longhd/.cargo/advisory-db)
    Updating crates.io index
    Scanning Cargo.lock for vulnerabilities (358 crate dependencies)
Crate:     rsa
Version:   0.10.0-rc.18
Title:     Marvin Attack: potential key recovery through timing sidechannels
Date:      2023-11-22
ID:        RUSTSEC-2023-0071
URL:       https://rustsec.org/advisories/RUSTSEC-2023-0071
Severity:  5.9 (medium)
Solution:  Upgrade to >=0.10.0 (patched)

error: 1 vulnerability found!
exit code: 1
```

The fix is in `rsa` 0.10.0, but `russh` 0.65.1 (the latest release) requires
exactly `rsa =0.10.0-rc.18`, so it cannot be selected. The advisory concerns
private-key operations; in vssh that is signing with an RSA key **file**. Keys
held by an agent, and Ed25519/ECDSA keys, do not use this code. I chose to
ignore it in `.cargo/audit.toml` (with the reason) so the audit job stays
useful for new advisories; removing that entry makes the job fail until russh
updates. The other option is building russh without its `rsa` feature, which
drops RSA keys entirely. That is a decision for the maintainer.

## Real output

### `cargo fmt --check`

```
exit code: 0
```

### `cargo build --release` (after `cargo clean --release`)

```
   Compiling libc v0.2.191
   Compiling cfg-if v1.0.5
   Compiling subtle v2.6.1
   ... (222 more "Compiling" lines elided) ...
   Compiling russh v0.65.1
   Compiling rusqlite v0.43.0
   Compiling vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `release` profile [optimized] target(s) in 1m 46s
exit code: 0
```

228 crates compiled, 0 warnings.

```
-rwxrwxr-x 2 longhd longhd 6202360 Nov 16 16:14 target/release/vssh
vssh 0.1.0
```

### `cargo test`

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.76s
     Running unittests src/lib.rs (target/debug/deps/vssh-ec2a087e5b7d25dc)

running 38 tests
test config::tests::partial_config_keeps_defaults ... ok
test config::tests::rejects_typos_and_bad_colours ... ok
test ssh::forward::tests::socks5_connect_requests ... ok
test ssh::forward::tests::socks5_rejects_what_it_cannot_do ... ok
test ssh::tests::known_key_types_are_offered_first ... ok
test ssh::tests::key_of_another_type_does_not_count_as_unknown ... ok
test ssh::tests::host_key_policy ... ok
test store::ssh_config::tests::first_host_in_empty_file ... ok
test ssh::tests::target_resolution ... ok
test store::ssh_config::tests::glob_patterns ... ok
test store::ssh_config::tests::forwards_parse_and_format ... ok
test store::ssh_config::tests::line_breaks_cannot_inject_directives ... ok
test store::ssh_config::tests::lists_only_concrete_hosts ... ok
test store::ssh_config::tests::multi_valued_add_remove ... ok
test store::ssh_config::tests::round_trip_is_lossless ... ok
test ui::draw::tests::chips_are_stable_and_popups_fit ... ok
test store::ssh_config::tests::parses_separators_quotes_and_case ... ok
test store::ssh_config::tests::host_lifecycle ... ok
test ui::files::tests::remote_paths ... ok
test ui::draw::tests::formats_sizes_and_ages ... ok
test store::ssh_config::tests::resolves_like_openssh ... ok
test store::ssh_config::tests::set_replaces_adds_and_removes ... ok
test ui::tests::clean_strips_escape_sequences ... ok
test ui::term::tests::encodes_keys_like_xterm ... ok
test ui::tests::tag_filter_matches_tags_and_group_by_prefix ... ok
test ui::term::tests::answers_terminal_queries ... ok
test ui::term::tests::renders_text_colours_and_wide_chars ... ok
test vault::tests::malformed_plaintext_is_rejected_not_trusted ... ok
test ui::files::tests::local_pane_operations ... ok
test store::db::tests::metadata_and_snippets ... ok
test store::ssh_config::tests::save_backs_up_once_and_is_private ... ok
test vault::tests::set_takes_the_secret_out_of_the_caller ... ok
test vault::tests::file_is_private_opaque_and_renonced ... ok
test vault::tests::create_refuses_to_overwrite_or_use_empty_passphrase ... ok
test vault::tests::debug_and_errors_never_show_secrets ... ok
test vault::tests::wrong_passphrase_is_rejected ... ok
test vault::tests::round_trip ... ok
test vault::tests::tampering_is_detected ... ok

test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s

     Running unittests src/main.rs (target/debug/deps/vssh-a08097de3898dc4f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/security.rs (target/debug/deps/security-20489d7f43bd4846)

running 11 tests
test comments_and_includes_are_inert ... ok
test db_module_builds_no_sql_from_strings ... ok
test edits_cannot_inject_blocks_or_directives ... ok
test pathological_patterns_do_not_hang ... ok
test hostile_host_line_stays_in_its_own_block ... ok
test identity_file_expands_only_a_leading_home ... ok
test vault_debug_and_errors_do_not_leak ... ok
test vault_file_stays_private_and_unique ... ok
test parser_never_panics_and_keeps_its_meaning ... ok
test vault_rejects_anything_that_is_not_an_intact_vault ... ok
test sql_metacharacters_are_stored_as_data ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s

   Doc-tests vssh

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit code: 0
```

49 tests: 38 in-crate, 11 in `tests/security.rs`. With the guards for items 1
and 4 above temporarily disabled, the corresponding tests fail (checked), so
they do test the fix.

### `cargo clippy -- -D warnings`

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.76s
exit code: 0
```

### `cargo clippy --all-targets -- -D warnings` (what CI runs)

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.79s
exit code: 0
```

### `cargo audit` (cargo-audit 0.23.0, with `.cargo/audit.toml`)

```
    Fetching advisory database from `https://github.com/RustSec/advisory-db.git`
      Loaded 1509 security advisories (from /home/longhd/.cargo/advisory-db)
    Updating crates.io index
    Scanning Cargo.lock for vulnerabilities (358 crate dependencies)
exit code: 0
```

`cargo-audit` was not installed on this machine; it was installed into a
temporary directory for this run and removed afterwards. `cargo deny` was not
added: the brief makes it conditional on a deny config, and there is none.

## Coverage of the brief's test list

| Asked | Where |
|-------|-------|
| Vault: wrong passphrase, bit flips in magic/salt/nonce/ciphertext/tag, mode 0600, fresh nonce, refuses overwrite, empty passphrase | `src/vault/mod.rs` (existing) |
| Vault: malformed/truncated files, loosened permissions restored, no temp file left, distinct salt per vault | `tests/security.rs` |
| Vault: Debug output and error messages never contain a secret or passphrase | `src/vault/mod.rs`, `tests/security.rs` |
| ssh-config: hostile `Host` tokens, `~` expansion, comments, `Include`, no leaking between hosts | `tests/security.rs` |
| ssh-config: no panic on garbage (3000 random token documents through every public function), no hang on pathological patterns | `tests/security.rs` |
| Host key policy: unknown / known / mismatch / other key type / port and name separation | `src/ssh/mod.rs` |
| SQL: metacharacters stored as data through add/edit/rename/delete; no string-built SQL in `db.rs` | `tests/security.rs` |
| No secret in logs | enforced by clippy (`print_stdout`, `print_stderr`, `dbg_macro` denied); vssh has no logging |

## Not verified in this round

- The workflow on GitHub itself (not pushed).
- Startup time was not re-measured after these changes; the release binary is now 6,202,360 bytes.
- The full end-to-end pass from the first round was not repeated. Live checks
  this round covered only: host key verification (three cases), the dead-tab
  behaviour, and the `#tag` filter.
- No test drives the TUI key handling automatically; fix 3 was verified by hand in tmux.
- The Vietnamese README text was written by me and not reviewed by a native speaker.

## Commits in this round

```
30741f0 docs: bilingual README (English + Vietnamese)
c469a5d ci: GitHub Actions for fmt, build, test, clippy and cargo audit
8a4d5d1 fix(ui): a dead session closes only with leader x, not on any key
a8722cb test: security integration suite
eb8b2af build: deny print/dbg macros in clippy, optimize argon2 in dev profile
0dd7281 feat(ui): '#tag' in the search box filters by tag or group
8f6aa3e fix(store): harden the ssh config writer and pattern matcher
c181126 feat(vault): redacting Debug impl, tests for leaks and malformed contents
76da434 fix(ssh): refuse a known host that presents a key of another type
34d55bd fix(deps): rusqlite 0.43 for RUSTSEC-2026-0459 in bundled SQLite
b46324a refactor: split into library and thin binary
```

`BRIEF-CI.md` is still untracked; it was read and executed but not committed.


---

# Update: UI restyle, quick connect, README media (BRIEF-UI.md)

Run on 2026-11-16 on the same machine, at commit `3acf538`. Output is pasted
as produced. Test count and binary size here supersede the earlier sections.

## What changed

| Brief item | Result |
|------------|--------|
| Two-tier host rows | Line 1: status dot, pin, alias (bold), latency right-aligned. Line 2, quieter: `user@host[:port] · group` and tag badges. Badge colour is a hash of the name over a fixed 8-colour palette. A long address is shortened with `…` before group or tags are dropped. |
| Detail panel | Sections CONNECTION (address, port, user, key), ORGANISATION (group, tags, notes), ACTIVITY (last used, sessions, vault), FORWARDS, under a summary line with address and reachability. |
| Status dots | Green with connect time (`<1 ms` when it rounds to zero), red `down`, hollow gray when not checked. The red state covers refused as well as timed-out connections, so it is labelled `down`, not `timeout`. |
| Borders and focus | Thick accent border on the focused panel, thin rounded muted border on the others, one cell of padding in every panel. |
| Header and bottom bar | Header: `vssh` badge, tabs, and a breadcrumb on the hosts tab (`hosts › web-01 › forwards`, `hosts › /#web`), plus marked / forwarding / vault state on the right. Bottom bar: keys of whatever has focus, key names highlighted. |
| Theme, backwards compatible | New entries `warn`, `bar`, `key` and key `connect`, all with defaults. A config written before this change is covered by a unit test (`config_from_before_the_restyle_still_loads`). |
| Quick connect | `c`: form for `user@host[:port]` and an optional password, opens a session tab. |
| README media | `docs/img/demo.gif` and four screenshots, embedded in both language sections. |

All existing keys still do what they did; nothing was rebound.

## Quick connect: checked live

Against the demo container with an empty throwaway HOME (so no key could log in):

- malformed input is rejected in the form (`expected user@host or user@host:port`);
- a wrong password leads to the password prompt, the right one then logs in (`sshd` log: `Failed password`, then `Accepted password for root`);
- afterwards the HOME contained only `.ssh/known_hosts` and vssh's database, with 0 host rows and 0 snippet rows; no `~/.ssh/config`, no vault file.

`known_hosts` is written on purpose: the host key is verified for quick connects exactly as for saved hosts.

## README media

Recorded with vhs 0.12.1 (downloaded to a temp directory together with ttyd; the
system Chrome and ffmpeg were used) by `docs/img/demo.sh`, which was run from a
clean state for the committed files. The app in the pictures is the release
binary; nothing is drawn or edited.

What is staged in them: the six hosts are demo entries. Four point at one local
Alpine container (`*.localhost:2222`), two at ports where nothing listens, to
show the `down` state. Groups, tags, notes, pins and snippets are inserted into
the app's database by the script. "Sessions 2" and "Last used" come from two
real sessions the tape opens off camera. Latencies are real loopback numbers.
Outbound port 22 is blocked on this network, so no internet host could be shown.

## Real output

### `cargo fmt --check`

```
exit code: 0
```

### `cargo build --release` (after `cargo clean --release`)

```
   Compiling libc v0.2.191
   Compiling cfg-if v1.0.5
   Compiling subtle v2.6.1
   ... (222 more "Compiling" lines elided) ...
   Compiling russh v0.65.1
   Compiling rusqlite v0.43.0
   Compiling vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `release` profile [optimized] target(s) in 2m 25s
exit code: 0
```

228 crates compiled, 0 warnings.

```
-rwxrwxr-x 2 longhd longhd 6235640 Nov 16 17:21 target/release/vssh
vssh 0.1.0
```

### `cargo test`

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.24s
     Running unittests src/lib.rs (target/debug/deps/vssh-ec2a087e5b7d25dc)

running 40 tests
test config::tests::config_from_before_the_restyle_still_loads ... ok
test ssh::tests::host_key_policy ... ok
test config::tests::partial_config_keeps_defaults ... ok
test ssh::tests::key_of_another_type_does_not_count_as_unknown ... ok
test config::tests::rejects_typos_and_bad_colours ... ok
test ssh::tests::known_key_types_are_offered_first ... ok
test ssh::forward::tests::socks5_rejects_what_it_cannot_do ... ok
test store::ssh_config::tests::glob_patterns ... ok
test store::ssh_config::tests::lists_only_concrete_hosts ... ok
test store::ssh_config::tests::multi_valued_add_remove ... ok
test ssh::forward::tests::socks5_connect_requests ... ok
test ssh::tests::target_resolution ... ok
test store::ssh_config::tests::forwards_parse_and_format ... ok
test ssh::tests::quick_connect_targets ... ok
test store::ssh_config::tests::parses_separators_quotes_and_case ... ok
test store::ssh_config::tests::first_host_in_empty_file ... ok
test store::ssh_config::tests::line_breaks_cannot_inject_directives ... ok
test store::ssh_config::tests::set_replaces_adds_and_removes ... ok
test store::ssh_config::tests::host_lifecycle ... ok
test ui::draw::tests::formats_sizes_and_ages ... ok
test ui::draw::tests::chips_are_stable_and_popups_fit ... ok
test ui::term::tests::answers_terminal_queries ... ok
test ui::term::tests::renders_text_colours_and_wide_chars ... ok
test ui::tests::clean_strips_escape_sequences ... ok
test ui::term::tests::encodes_keys_like_xterm ... ok
test ui::tests::tag_filter_matches_tags_and_group_by_prefix ... ok
test store::ssh_config::tests::round_trip_is_lossless ... ok
test store::ssh_config::tests::resolves_like_openssh ... ok
test store::db::tests::metadata_and_snippets ... ok
test ui::files::tests::remote_paths ... ok
test ui::files::tests::local_pane_operations ... ok
test vault::tests::malformed_plaintext_is_rejected_not_trusted ... ok
test store::ssh_config::tests::save_backs_up_once_and_is_private ... ok
test vault::tests::set_takes_the_secret_out_of_the_caller ... ok
test vault::tests::file_is_private_opaque_and_renonced ... ok
test vault::tests::debug_and_errors_never_show_secrets ... ok
test vault::tests::create_refuses_to_overwrite_or_use_empty_passphrase ... ok
test vault::tests::round_trip ... ok
test vault::tests::wrong_passphrase_is_rejected ... ok
test vault::tests::tampering_is_detected ... ok

test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s

     Running unittests src/main.rs (target/debug/deps/vssh-a08097de3898dc4f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/security.rs (target/debug/deps/security-20489d7f43bd4846)

running 11 tests
test comments_and_includes_are_inert ... ok
test db_module_builds_no_sql_from_strings ... ok
test identity_file_expands_only_a_leading_home ... ok
test pathological_patterns_do_not_hang ... ok
test hostile_host_line_stays_in_its_own_block ... ok
test edits_cannot_inject_blocks_or_directives ... ok
test vault_debug_and_errors_do_not_leak ... ok
test vault_file_stays_private_and_unique ... ok
test vault_rejects_anything_that_is_not_an_intact_vault ... ok
test parser_never_panics_and_keeps_its_meaning ... ok
test sql_metacharacters_are_stored_as_data ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

   Doc-tests vssh

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit code: 0
```

51 tests: 40 in-crate, 11 in `tests/security.rs`.

### `cargo clippy -- -D warnings`

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.12s
exit code: 0
```

### `cargo clippy --all-targets -- -D warnings` (what CI runs)

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.21s
exit code: 0
```

## Not done / not verified

- Rendering is checked by looking at the recorded screenshots, not by automated tests; only the helpers (shortening, latency label, colour hashing, config defaults, target parsing) have unit tests.
- Only one terminal theme and size was captured (1280x680, Catppuccin Mocha). Light terminal backgrounds and very narrow windows were not looked at; the new `bar` default is a dark colour.
- The help popup and the forwards / snippets panels got the new panel style but no layout redesign.
- Startup time was not re-measured.
- `cargo audit` was not re-run in this round (no dependency changed).

## Commits in this round

```
3acf538 docs: README shows the new UI and documents quick connect
4924a19 docs(img): README media recorded from the real app with vhs
9bc61b6 feat(ui): restyle the TUI
2caab58 feat: quick connect without saving, new theme entries
```

`BRIEF-UI.md` and `BRIEF-CI.md` are untracked and left that way. A `plans/`
directory appeared in the repo during this work; it is not mine and is untouched.


---

# Update: broadcast, split layouts, LazyVim-style UI (BRIEF-BROADCAST.md)

Run on 2026-11-16 on branch `feat/broadcast-lazyvim`, at commit `0ee4c44`. Not
pushed. Output is pasted as produced. Test count and binary size here
supersede the earlier sections.

## What the brief assumed that was not there

- `install.sh` did not exist. It was written from scratch.
- There was no which-key popup. It is new in this round.
- Snippets were never typed into the active tab; they only ran over a separate
  exec connection. `leader s` now types one into the session.

## Milestones

| Milestone | Commit | Result |
|-----------|--------|--------|
| M1 broadcast core | `100e868` | `Space` + `Enter` opens every marked host; `T` connects every host with a tag or group; `Alt-b` / `leader b` sends typing and pastes to every live session, encoded per session. |
| M2 layouts | `51dc42a` | Single, 2 columns, 2x2 grid, 3 columns; `leader L` cycles; a sliding window keeps the focused session and its group on screen; `leader d` / `x` closes a pane. |
| M3 install language | `a6efafc` | `install.sh` in English or Vietnamese (`--lang`, `VSSH_LANG`, `LC_ALL`, `LC_MESSAGES`, `LANG`); TUI `lang` setting, `--lang`, `VSSH_LANG`. |
| M4 LazyVim UI | `c0e5ffb` | lualine-style status line with a coloured mode badge, grouped key hints with faded inactive keys, which-key popup with a window submenu, `Ctrl-d` / `Ctrl-u`, `Q`. |
| M5 parity + docs | `28c60b5`, `0ee4c44` | Session logging, login scripts (`[exec]` and `RemoteCommand`), snippets typed into sessions, README in both languages, PLAN with the audit and the not-planned list, new screenshots. |

## Decisions the brief left open or contradicted

1. **`leader l` cannot be both "focus right" and "toggle log".** The brief
   asks for pane focus on `h/j/k/l` and for `Ctrl-a l` to toggle logging. I
   gave `leader l` to logging, as the brief spells out twice, and put focus on
   `Alt-h/j/k/l` (only while several panes are shown), `leader` + arrow keys,
   and `leader w` + `h/j/k/l`.
2. **Existing leader keys moved.** `leader h` (host list) is now `leader q`;
   `leader k` / `j` (scroll) are now `leader PgUp` / `PgDn` or
   `leader Ctrl-u` / `Ctrl-d`.
3. **`Alt-b` no longer reaches the remote shell**, where readline uses it for
   "back one word". The brief asks for it as the broadcast toggle.
4. **Broadcast reaches sessions that are not on screen.** The brief says all
   connected sessions. The status line always shows how many are listening.
5. **Panes are session tabs shown together**, not a separate pane tree, so
   every pane is also in the tab bar and all tab keys keep working.
6. **`q` still quits from the host list** (existing, configurable); `Q` quits
   from the host list and SFTP; `leader q` leaves the session view.
7. **TUI translation is partial**: key hints, leader menu, help screen and
   status-line words. Forms, confirmations and error messages stay English
   because those strings are not centralised.
8. **Session logs are raw terminal output** (escape sequences included, like
   `script`), starting with a plain-text copy of what is on screen.
9. **`[exec]` is typed into the shell; `RemoteCommand` replaces the shell.**
   That is how OpenSSH treats `RemoteCommand`; the typed command is echoed
   once before the prompt appears.
10. **Not done, marked optional in the brief:** per-pane broadcast switch,
    pane resize with `<` / `>`. `gg` is the existing single `g`.

## Checked live against a real SSH server

A throwaway Alpine container on loopback and a temporary HOME, driven through
tmux:

- three marked hosts, `Enter`: three sessions in three columns;
- `Alt-b`, one typed `echo pid-$$`: three different PIDs, one per pane;
  broadcast off, typing reaches the focused pane only;
- `leader L` cycles the four layouts; `leader w g` jumps to the grid;
  `Alt-l` / `Alt-h`, `leader` + arrow and `leader w h` move focus; in the
  single layout `Alt-l` is left to the shell;
- `T` with a tag connects two hosts, with a group three, with an unknown tag
  it reports `no host has the tag or group nope`; a second group opened later
  is shown together, not split across pages;
- `leader d` closes the focused pane and focus stays with the sessions;
- `[exec]` runs `echo login-script-$((5+5))` on connect (`login-script-10`);
  a host with `RemoteCommand echo remote-command-ran; sleep 2` prints it and
  the session ends;
- `leader l` writes `logs/a1-20261116-211044.log` (file 0600, directory 0700)
  containing the screen at that moment and everything after;
- `leader s` lists the snippets and types the chosen one (`whoami` → `root`);
- `--lang vi`: Vietnamese hints, leader menu and help; `Ctrl-d` / `Ctrl-u`
  move the selection; `Q` with an open tab asks before quitting.

`install.sh` was run for real into a temporary prefix: from the checkout in
Vietnamese and in English (the second run updating the first), and from
outside a checkout with `cargo` hidden, where it reports that no release
binary exists and that Rust is needed, in the chosen language, exit code 1.

## Real output

### `cargo fmt --check`

```
exit code: 0
```

### `cargo build --release` (after `cargo clean --release`)

```
   Compiling libc v0.2.191
   Compiling cfg-if v1.0.5
   Compiling subtle v2.6.1
   ... (222 more "Compiling" lines elided) ...
   Compiling russh v0.65.1
   Compiling rusqlite v0.43.0
   Compiling vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `release` profile [optimized] target(s) in 1m 43s
exit code: 0
```

228 crates compiled, 0 warnings.

```
-rwxrwxr-x 2 longhd longhd 6314488 Nov 16 21:19 target/release/vssh
vssh 0.1.0
```

### `cargo test`

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.13s
     Running unittests src/lib.rs (target/debug/deps/vssh-ec2a087e5b7d25dc)

running 54 tests
test config::tests::partial_config_keeps_defaults ... ok
test config::tests::login_scripts_and_logging ... ok
test config::tests::language_setting ... ok
test config::tests::config_from_before_the_restyle_still_loads ... ok
test config::tests::rejects_typos_and_bad_colours ... ok
test ssh::forward::tests::socks5_connect_requests ... ok
test ssh::forward::tests::socks5_rejects_what_it_cannot_do ... ok
test ssh::tests::known_key_types_are_offered_first ... ok
test ssh::tests::key_of_another_type_does_not_count_as_unknown ... ok
test ssh::tests::login_script_is_typed_line_by_line ... ok
test ssh::tests::quick_connect_targets ... ok
test store::ssh_config::tests::first_host_in_empty_file ... ok
test store::ssh_config::tests::forwards_parse_and_format ... ok
test ssh::tests::remote_command_comes_from_the_ssh_config ... ok
test store::ssh_config::tests::glob_patterns ... ok
test ssh::tests::host_key_policy ... ok
test store::ssh_config::tests::line_breaks_cannot_inject_directives ... ok
test store::ssh_config::tests::lists_only_concrete_hosts ... ok
test store::ssh_config::tests::multi_valued_add_remove ... ok
test ssh::tests::target_resolution ... ok
test store::ssh_config::tests::round_trip_is_lossless ... ok
test store::ssh_config::tests::parses_separators_quotes_and_case ... ok
test ui::draw::tests::chips_are_stable_and_popups_fit ... ok
test store::ssh_config::tests::resolves_like_openssh ... ok
test store::ssh_config::tests::host_lifecycle ... ok
test ui::panes::tests::focus_moves_like_vim_windows ... ok
test ui::panes::tests::layouts_cycle_and_fit ... ok
test ui::draw::tests::formats_sizes_and_ages ... ok
test ui::files::tests::remote_paths ... ok
test store::ssh_config::tests::set_replaces_adds_and_removes ... ok
test ui::panes::tests::window_follows_the_active_session ... ok
test ui::term::tests::encodes_keys_like_xterm ... ok
test ui::term::tests::answers_terminal_queries ... ok
test ui::term::tests::renders_text_colours_and_wide_chars ... ok
test ui::tests::clean_strips_escape_sequences ... ok
test ui::tests::connect_by_tag_is_exact ... ok
test ui::tests::broadcast_encodes_for_each_sessions_own_mode ... ok
test ui::tests::a_picked_snippet_is_typed_into_every_broadcast_session ... ok
test ui::tests::tag_filter_matches_tags_and_group_by_prefix ... ok
test vault::tests::malformed_plaintext_is_rejected_not_trusted ... ok
test ui::tests::broadcast_sends_keys_to_every_live_session ... ok
test store::db::tests::metadata_and_snippets ... ok
test ui::panes::tests::geometry_of_each_preset ... ok
test ui::panes::tests::panes_tile_the_area_without_gaps_or_overlap ... ok
test ui::files::tests::local_pane_operations ... ok
test ui::tests::session_logs_are_private_and_stay_in_their_directory ... ok
test store::ssh_config::tests::save_backs_up_once_and_is_private ... ok
test vault::tests::set_takes_the_secret_out_of_the_caller ... ok
test vault::tests::debug_and_errors_never_show_secrets ... ok
test vault::tests::file_is_private_opaque_and_renonced ... ok
test vault::tests::create_refuses_to_overwrite_or_use_empty_passphrase ... ok
test vault::tests::wrong_passphrase_is_rejected ... ok
test vault::tests::round_trip ... ok
test vault::tests::tampering_is_detected ... ok

test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

     Running unittests src/main.rs (target/debug/deps/vssh-a08097de3898dc4f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/install.rs (target/debug/deps/install-0657114a08835ab6)

running 1 test
test installer_picks_its_language ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

     Running tests/security.rs (target/debug/deps/security-20489d7f43bd4846)

running 11 tests
test comments_and_includes_are_inert ... ok
test hostile_host_line_stays_in_its_own_block ... ok
test db_module_builds_no_sql_from_strings ... ok
test pathological_patterns_do_not_hang ... ok
test edits_cannot_inject_blocks_or_directives ... ok
test identity_file_expands_only_a_leading_home ... ok
test vault_debug_and_errors_do_not_leak ... ok
test vault_file_stays_private_and_unique ... ok
test parser_never_panics_and_keeps_its_meaning ... ok
test vault_rejects_anything_that_is_not_an_intact_vault ... ok
test sql_metacharacters_are_stored_as_data ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s

   Doc-tests vssh

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit code: 0
```

66 tests: 54 in-crate, 1 for the installer, 11 in `tests/security.rs`. The
tests the brief asks for: broadcast routing (`broadcast_sends_keys_to_every_live_session`,
`broadcast_encodes_for_each_sessions_own_mode`), layout geometry
(`ui::panes::tests`), installer language detection (`tests/install_lang.sh`,
run by `tests/install.rs`), login script (`login_script_is_typed_line_by_line`,
`remote_command_comes_from_the_ssh_config`, `login_scripts_and_logging`).

### `sh tests/install_lang.sh` (18 cases; also passes under `dash` and `bash`)

```
install.sh language selection: ok
exit code: 0
```

### `cargo clippy -- -D warnings`

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.85s
exit code: 0
```

### `cargo clippy --all-targets -- -D warnings` (what CI runs)

```
    Checking vssh v0.1.0 (/home/longhd/projects/vssh)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.08s
exit code: 0
```

## README media

vhs could not be used this round (fetching its binary was blocked by a
permission check), so `docs/img/shots.py` captures the real app with tools
already installed: `tmux capture-pane -e` for the screen and its colours,
headless Chrome to render each capture to PNG, ffmpeg to join the frames into
`demo.gif`. The GIF is therefore a sequence of real screens, 1.6 s each, not a
smooth recording. The vhs tape was removed. Demo data is as before: six demo
hosts, four on one local container, two on dead ports.

## Not done / not verified

- The release-download path of `install.sh` has never succeeded: no GitHub
  release exists, and no release workflow was added. The `cargo install --git`
  fallback was not run either (it would build the pushed default branch, not
  this one).
- No automated test drives the key handling of the TUI itself; the routing,
  geometry and encoding it relies on are unit-tested, the rest was checked by
  hand in tmux as listed above.
- Sessions were only tested against one container playing several hosts, so
  broadcast was never seen across genuinely different machines.
- Startup time and `cargo audit` were not re-run; no dependency changed.
- The Vietnamese strings were written by me and not reviewed by a native speaker.
- The binary in `~/.local/bin` is still the previous build; it was not replaced this round.

## Commits in this round

```
0ee4c44 docs: M5 README (EN + VI), plan, and fresh screenshots
28c60b5 feat: M5 session logging, login scripts, snippets typed into sessions
c0e5ffb feat: M4 LazyVim-style status line, grouped hints, which-key menu
a6efafc feat: M3 bilingual installer and language setting
51dc42a feat: M2 split layouts for sessions
100e868 feat: M1 broadcast core, connect to many hosts at once
```

`BRIEF-BROADCAST.md` is untracked and left that way.
