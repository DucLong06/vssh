# vssh

A terminal SSH connection manager: host list on the left, details or a live
session on the right, vim keys throughout. Hosts live in your normal
`~/.ssh/config`, so plain `ssh` keeps working with everything vssh edits.

- Host management on top of `~/.ssh/config` (add, edit, duplicate, delete),
  with groups, tags, notes, pins and usage stats in SQLite
- SSH sessions in tabs, embedded terminal emulator, any number at once
- Dual-pane SFTP browser with progress
- Local, remote and dynamic (SOCKS5) port forwards, started and stopped per host
- Encrypted password vault (Argon2id + XChaCha20-Poly1305)
- Command snippets, run on one host or on every marked host
- Fuzzy search, TCP reachability check

Linux and macOS. Pure Rust SSH (russh), no `ssh` binary needed.

## Build and run

```sh
cargo build --release
./target/release/vssh
```

Needs Rust 1.89 or newer and a C compiler (for the bundled SQLite).

```sh
cargo test                    # unit tests
cargo clippy -- -D warnings
```

## Keys

Press `?` inside vssh for this list.

### Host list

| Key | Action |
|-----|--------|
| `j` / `k`, arrows | move |
| `g` / `G` | first / last |
| `Enter` | open a shell |
| `s` | open the SFTP browser |
| `/` | fuzzy search (alias, hostname, user, group, tags); `Enter` keeps the filter, `Esc` clears it |
| `a` / `e` / `d` / `y` | add / edit / delete / duplicate host |
| `p` | pin to the top |
| `Space` | mark host (snippets run on all marked hosts) |
| `f` | port forwards of the host |
| `n` | snippets |
| `r` | check which hosts are reachable |
| `P` | store a password for the host in the vault |
| `V` | unlock / lock the vault |
| `Tab` / `Shift-Tab` | next / previous tab |
| `?` | help |
| `q` or `Ctrl-c` | quit (asks first if tabs or forwards are open) |

### Forwards and snippets panel

| Key | Action |
|-----|--------|
| `j` / `k` | move |
| `a` | add (`e` edits a snippet) |
| `d` | delete |
| `Enter` | start/stop the forward, or run the snippet |
| `Esc` | back to the host list |

A forward is typed the way `ssh` flags read: `L 8080 localhost:80`,
`R 9000 localhost:3000`, `D 1080`, optionally with a bind address
(`L 0.0.0.0:8080 localhost:80`). They are saved as `LocalForward`,
`RemoteForward` and `DynamicForward` lines in the ssh config.

### Inside a tab

In a shell every key goes to the remote machine, so tab control sits behind a
leader key, `Ctrl-a` by default.

| Key | Action |
|-----|--------|
| `Ctrl-a` `Tab` or `n` | next tab |
| `Ctrl-a` `p` | previous tab |
| `Ctrl-a` `1`-`9` | go to tab |
| `Ctrl-a` `h` | host list |
| `Ctrl-a` `x` | close tab |
| `Ctrl-a` `k` / `j` | scroll back / forward |
| `Ctrl-a` `Ctrl-a` | send a literal `Ctrl-a` |

### SFTP browser

Left pane is local, right pane is remote.

| Key | Action |
|-----|--------|
| `Tab` | switch pane |
| `j` / `k`, `g` / `G` | move |
| `Enter` or `l` | open directory |
| `h` or `Backspace` | parent directory |
| `c` | copy the file to the other pane (asks before overwriting) |
| `m` / `r` / `d` | mkdir / rename / delete |
| `R` | refresh |
| `q` | close |

## Authentication

Tried in this order: SSH agent (keys named by `IdentityFile` first), key files
(`IdentityFile`, or `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`), then the
password stored in the vault. If all are refused, vssh asks for a password
once, without storing it.

Host keys are checked against `~/.ssh/known_hosts`. A host seen for the first
time is recorded (like `StrictHostKeyChecking accept-new`); a host whose key
changed is refused.

## Vault

`P` on a host stores its password. The first use asks for a new master
passphrase. The vault is one file, `~/.local/share/vssh/vault`, mode 0600:
Argon2id (19 MiB, 2 passes) derives the key, XChaCha20-Poly1305 encrypts, each
save uses a fresh nonce. There is no recovery if the passphrase is lost.

Entries are keyed by host alias. Renaming or deleting a host leaves its entry
behind; store the password again under the new name.

## Files

| Path | Contents |
|------|----------|
| `~/.ssh/config` | hosts and forwards; the pre-vssh version is kept once as `config.vssh.bak` |
| `~/.ssh/known_hosts` | host keys, shared with OpenSSH |
| `~/.config/vssh/config.toml` | theme and keys (optional) |
| `~/.local/share/vssh/vssh.db` | groups, tags, notes, pins, stats, snippets |
| `~/.local/share/vssh/vault` | encrypted passwords |

## Configuration

`~/.config/vssh/config.toml`, every entry optional. Defaults shown:

```toml
ping_on_start = false   # TCP-check every host at launch

[theme]                 # colour names or "#rrggbb"
accent = "cyan"
border = "darkgray"
text = "reset"
dim = "darkgray"
selection = "#264f78"
ok = "green"
error = "red"

[keys]                  # one character each; leader is pressed with Ctrl
leader = "a"
quit = "q"
search = "/"
sftp = "s"
forwards = "f"
snippets = "n"
add = "a"
edit = "e"
delete = "d"
duplicate = "y"
pin = "p"
mark = " "
ping = "r"
password = "P"
vault = "V"
help = "?"
```

Only the leader and the host-list keys can be remapped.

## Not supported yet

- `ProxyJump` / `ProxyCommand` (such hosts are shown but refuse to connect)
- `Include` in the ssh config (the line is preserved, the files are not read)
- Passphrase-protected key files (load them into the agent)
- keyboard-interactive / 2FA logins
- Copying directories over SFTP (files only)
- Telnet and serial (stub module only)

See [docs/PLAN.md](docs/PLAN.md) for the design and milestones.
