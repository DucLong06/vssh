#!/bin/sh
# GUI smoke test, run by hand: the real desktop app over a throwaway HOME
# with four hosts, on a virtual X display. It opens the window, waits for the
# hosts to be read, opens the palette with Ctrl-K, finds a host and chooses
# it. Screenshots of the steps land in docs/img/desktop-d1*.png.
#
# shortcut: the window is driven blind with xdotool, and what is asserted is
# what the app leaves behind (its window, its database, a live process), not
# the DOM; look at the screenshots for that. Upgrade to tauri-driver once
# WebKitWebDriver is installed (apt: webkit2gtk-driver).
set -eu
cd "$(dirname "$0")/.."
SIZE=1100x700

if [ -z "${VSSH_SMOKE_DISPLAY:-}" ]; then
    for tool in xvfb-run xdotool ffmpeg npm; do
        command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed"; exit 1; }
    done
    # `tauri build` puts the frontend inside the binary; a plain `cargo
    # build` makes one that looks for the dev server.
    (cd crates/vssh-desktop && npm run tauri build -- --debug --no-bundle)
    exec env VSSH_SMOKE_DISPLAY=1 xvfb-run -a -s "-screen 0 ${SIZE}x24" "$0"
fi

home=$(mktemp -d)
app=
trap 'kill "$app" 2>/dev/null; rm -rf "$home"' EXIT
mkdir -p "$home/.ssh" "$home/.local/share"
# The user's own fonts (Maple Mono NF) are under the HOME this run hides.
[ -d "$HOME/.local/share/fonts" ] && ln -s "$HOME/.local/share/fonts" "$home/.local/share/fonts"
cat >"$home/.ssh/config" <<'EOF'
Host web-01
    HostName 10.0.0.5
    User deploy
    Port 2222
Host web-02
    HostName 10.0.0.6
    User deploy
Host db-primary
    HostName db.internal.example.com
    User postgres
Host bare
EOF

shot() {
    sleep 1
    ffmpeg -loglevel error -y -f x11grab -draw_mouse 0 -video_size "$SIZE" -i "$DISPLAY" -frames:v 1 "docs/img/$1"
    echo "shot: docs/img/$1"
}
step() { echo "ok: $1"; }

# No GPU on a virtual display.
HOME="$home" WEBKIT_DISABLE_DMABUF_RENDERER=1 target/debug/vssh-desktop &
app=$!

win=$(timeout 60 xdotool search --sync --onlyvisible --name '^vssh$' | head -n 1)
[ -n "$win" ] || { echo "FAIL: no window named vssh after 60 s"; exit 1; }
step "the window opened"

# list_hosts imports the config's hosts into the database: the file is there
# once the page has loaded and its first command has been answered.
tries=0
until [ -f "$home/.local/share/vssh/vssh.db" ]; do
    tries=$((tries + 1))
    [ "$tries" -le 60 ] || { echo "FAIL: the window never asked for the hosts"; exit 1; }
    sleep 0.5
done
step "the page read the hosts of the fixture config"
shot desktop-d1.png

xdotool windowfocus "$win"
xdotool key --window "$win" ctrl+k
xdotool type --window "$win" db
shot desktop-d1-palette.png

xdotool key --window "$win" Return
shot desktop-d1-selected.png

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the palette chose a host"
echo "gui smoke: ok (now look at the screenshots)"
