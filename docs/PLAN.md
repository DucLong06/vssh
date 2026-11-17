# vssh design notes

This file starts with the status bar work (brief 8). Notes on earlier
milestones were never committed; the code comments and the READMEs are the
record for those.

## Status bar v2

### Server stats

- **One command, one round.** `ui/stats.rs` holds the command, the parser and
  the formatter. The command is wrapped in `sh -c` so a fish or csh login
  shell does not have to parse it. Linux reads `/proc/loadavg`,
  `/proc/cpuinfo` and `/proc/meminfo`; anything without `/proc` falls back to
  `uptime`, `sysctl` and `vm_stat` (macOS). `df -Pk /` is the same everywhere.
  The lines the script prints itself are labelled (`load`, `cpus`,
  `membytes`), so the parser never guesses which tool printed what, and
  unknown lines (a banner, `[exit status 1]`) are skipped.
- **CPU is load per core**, not a `/proc/stat` delta: it needs no second
  sample, so the very first answer (and a one-off probe) already has it.
- **No second connection for a session.** `ssh::shell` borrows the connection
  handle; the tab's task runs the shell and `stats::poll` side by side with
  `select!`. Each round is one exec channel with a 4 second timeout, then a
  5 second sleep. Three failures in a row (a server with `MaxSessions 1`, a
  forced command) and the session stops asking.
- **Never blocks rendering.** Results arrive as `Event::Stats` like every
  other background result and land in `App::stats`, keyed by tab title or
  host alias. The status line only reads that map.
- **Host list.** `App::probe_selected` runs once per frame and does nothing
  until the selection changes. Then it aborts the previous probe and starts a
  task that sleeps 700 ms (the debounce), connects, asks once and hangs up.
  The reachability check is a bare TCP connect, so there is no SSH channel of
  it to reuse; what is reused is a live session of that host, if there is one.
- **A probe is not a decision to connect.** It only goes to hosts whose key is
  already in `known_hosts` and it authenticates with the agent or key files
  only. Moving the cursor never records a new host key and never sends a
  vault password. Hosts marked down by the reachability check are skipped,
  and a host that refused a probe (it wants a password, say) is not probed
  again until a session to it has been opened and closed: each refusal is a
  failed login in that server's log.
- **Room.** The stats take what is free between the centred text and the
  tags; `Stats::compact` picks bars, then plain text, then fewer metrics.
  Outside the wide layout it returns nothing.

### Rounded everything

A terminal can only round a frame that is at least three rows high, so there
are two tools:

- `panel()` — the rounded border with one cell of padding. Every frame goes
  through it: host list, details, forwards, snippets, session panes, SFTP
  panes, which-key, help, confirm, snippet picker, forms.
- `pill()` — for one-line bars, where there is no room for a border: the text
  on a coloured band between two half circles (`◖ ◗`, or the Powerline caps
  with `nerd_font = true`). The caps stand where the square badge had its
  padding, so nothing moves and no width is lost. Used for the mode and the
  latency in the status line and for the `vssh` mark in the header.

The audit of every frame, and what changed:

| Where | Before | Now |
|-------|--------|-----|
| Status line: mode, latency | square badges | pills; the caps take the colour of what is next to them |
| Header: `vssh` mark | square badge | pill |
| Tab bar: active tab | bold text only | a band in the selection colour, one cell of space either side |
| Form fields (quick connect, add host, prompts) | bare text after the label | an input band per field, one cell of margin, cursor inside it |
| Key hints | one cell of margin, `│` between groups | unchanged |
| Details: section headers | accent text at the panel's one-cell margin | unchanged: a band here would read as a selected row |
| Host list, details, forwards, snippets | `panel()` | unchanged |
| Session panes | `panel()`, one free column between neighbours | unchanged |
| SFTP panes | `panel()`, one free column between them | unchanged |
| Search | part of the breadcrumb in the header, no frame | unchanged |
| Which-key, help, confirm, snippet picker | `panel()` over a cleared area | unchanged |

No double borders: side by side, frames keep one empty column between them.
Stacked frames (portrait, the 2x2 grid) touch, bottom edge on top edge; a
free row there would cost every pane a line of terminal.

### Transitions

- **What moves.** After a tab switch (any change of the active tab, a new
  session included) the body is drawn shifted right by 2 columns, then 1,
  then in place: three frames, 40 ms each. The brief lists the offsets as
  "1, 2, 0" and calls them shrinking; 2, 1, 0 is the shrinking order.
- **Only the drawing moves.** The shift is applied to the rectangle the view
  is drawn into, not to `App::body`, and the frames of a slide do not resize
  the remote PTYs: a slide never sends a window change to a server.
- **Timed redraws.** The main loop normally draws once per batch of events.
  While `App::slide` counts frames it waits for an event at most 40 ms and
  draws again, so the slide finishes on an idle screen too. The countdown
  reaches zero only together with the unshifted frame.
- **No flicker.** Nothing is cleared: ratatui diffs each frame against the
  last one and the alternate screen stays up.
- **Off switch.** `transitions = false`. There is no automatic latency check;
  the flag is the whole mechanism.

### Limits

- Every open session with `status_stats` on polls, not only the focused one,
  so switching tabs shows numbers at once. With dozens of sessions that is
  one small exec per session every 5 seconds.
- SFTP tabs do not poll; they show numbers only if the host list or a shell
  session of the same host has them.
- macOS parsing is covered by fixtures only, not by a run against a real Mac.

## Connection errors (brief 9, M6)

- **Tag at the source, classify in one place.** `ssh::connect` is three
  steps: `dial` (name lookup, then TCP to each address), `handshake` (SSH over
  the open stream, host key check included) and `login`. Each step puts a
  `conn_err::Fail` into the error it returns, with what it knew: the host that
  did not resolve, the `ip:port` that refused or timed out, how many keys and
  whether a password were tried, the fingerprint and the `known_hosts` line.
  `conn_err::classify` is the only place that reads it back.
- **Why `dial` resolves by itself.** A lookup failure and a connect failure
  are the same `io::Error` kind once they come out of
  `TcpStream::connect(name)`; telling them apart there would mean matching on
  message text. Resolving first makes the class a fact, and gives the address
  for the message.
- **No guessing.** Without a `Fail`, a known library type still gives the
  class (`io::ErrorKind`, `russh::Error`, tokio's `Elapsed`), but the message
  is the chain as it is. Anything else is `ERROR` with the chain as it is.
- **Two texts per failure.** `Fail`'s `Display` is English and goes into the
  chain; `Fail::message` is the line for people, in the interface language.
- **Where it shows.** A tab that never connected is closed and its report goes
  to the `! Connection failed` popup (several failures share it) and to
  `App::fails`, which the details pane shows until a connection succeeds. An
  auth failure opens the password prompt, titled with what was refused. A
  session that drops later keeps its tab, with the classified reason in it.

## Quick peek (brief 9, M1)

- **Shared module.** `ui/stats.rs` builds both commands from one script: the
  status line's `COMMAND`, and `PEEK`, which adds `uptime`, `uname -srm` and
  `hostname` as labelled lines. `parse_peek` runs the status line's `parse`
  for the metrics and reads the extra lines itself; it cannot fail, a line
  that is missing leaves its field empty and the popup shows a dash.
- **An explicit action, so a full connect**: unlike the host-list probe, the
  peek accepts a new host key and uses the vault password, as `Enter` would.
- **Cache.** `App::peeks` keeps the answer and when it came; 30 seconds of
  reuse, `r` forces a new request, and a request in flight is not doubled.
  A good answer also feeds the status line and clears the host's last failure.
- **Spinner.** While a host is being asked the main loop redraws every
  100 ms (`App::waiting`), the same timed wait the tab slide uses.

## OSC 52 copy (brief 9, M2)

- **No clipboard dependency.** `ui/term.rs` holds a twelve-line base64, the
  `ESC ] 52 ; c ; <base64> BEL` sequence, and `copy`, which writes it to
  stdout. Base64 also means copied text can never act as an escape sequence.
- **tmux.** With its default `set-clipboard external`, tmux drops OSC 52
  coming from a program. So under `$TMUX` the text is also piped to
  `tmux load-buffer -w -` (`-w` forwards to the outer terminal; without it on
  tmux older than 3.2).
- **`y` in a session is `Ctrl-a y`.** The brief asks for `y` in a session
  pane, but every bare key there belongs to the remote shell. The leader is
  the only place a session key can live. With that, the brief's "when not
  broadcasting" has nothing left to guard, and copy works either way.
- **Last command output** has no shell integration to lean on, so it is the
  rows between two identical prompts: the text left of the cursor, and the
  nearest row above that starts with it. No match (a clock in the prompt,
  text already typed) falls back to everything above the cursor. Visible
  screen only, and a wrapped command line counts as output.
- **`y` on the host list was duplicate.** Duplicate's default is now `Y`; a
  config that names `y` for duplicate still wins, because duplicate is
  matched first. The change errs on the harmless side: an old habit now
  copies a line instead of creating a host.
