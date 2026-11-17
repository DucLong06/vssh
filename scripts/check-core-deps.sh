#!/bin/sh
# vssh-core is the frontend-neutral half of vssh: the TUI and the desktop app
# both sit on it, so it must never pull in a terminal UI crate. Run by CI.
set -eu
cd "$(dirname "$0")/.."

BAD='ratatui|crossterm'
fail=0

# Every edge kind: a dev- or build-dependency would leak just as well.
tree=$(cargo tree -p vssh-core --edges normal,build,dev --prefix none)
if echo "$tree" | grep -E "^($BAD)[ -]"; then
    echo "FAIL: vssh-core depends on a TUI crate (above)"
    fail=1
fi
if grep -rnE "\b($BAD)::|extern crate ($BAD)" crates/vssh-core --include='*.rs'; then
    echo "FAIL: vssh-core names a TUI crate in its source (above)"
    fail=1
fi

# The other side of the line: the TUI asks vssh_core::task for background
# work and never drives the async runtime itself.
direct=$(cargo tree -p vssh --depth 1 --edges normal,build,dev --prefix none)
if echo "$direct" | grep -E "^tokio[ -]"; then
    echo "FAIL: the TUI depends on tokio directly (above); go through vssh_core::task"
    fail=1
fi

[ "$fail" = 0 ] && echo "core deps: ok"
exit "$fail"
