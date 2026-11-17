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

### Limits

- Every open session with `status_stats` on polls, not only the focused one,
  so switching tabs shows numbers at once. With dozens of sessions that is
  one small exec per session every 5 seconds.
- SFTP tabs do not poll; they show numbers only if the host list or a shell
  session of the same host has them.
- macOS parsing is covered by fixtures only, not by a run against a real Mac.
