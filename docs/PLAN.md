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
  vault password. Hosts marked down by the reachability check are skipped.
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

### Limits

- Every open session with `status_stats` on polls, not only the focused one,
  so switching tabs shows numbers at once. With dozens of sessions that is
  one small exec per session every 5 seconds.
- SFTP tabs do not poll; they show numbers only if the host list or a shell
  session of the same host has them.
- macOS parsing is covered by fixtures only, not by a run against a real Mac.
