#!/bin/sh
# GUI smoke test, run by hand: the real desktop app over a throwaway HOME, on
# a virtual X display, talking real SSH to a throwaway sshd on loopback.
#
# That sshd plays the fixture hosts: one port each, and a forced command that
# prints what such a host would print (scripts/smoke-hosts/<port>.txt). So
# nothing in the app is faked: the cards show what came through the core's
# connection, parser and error classes. It runs as you, on 127.0.0.1 and
# 127.0.0.2, with its own keys under the throwaway HOME, and is killed on exit.
#
#   web-01       healthy                       web-02      past every critical mark
#   db-primary   no meminfo, warnings          cache-01    nothing listens: REFUSED
#   db-replica   a name that does not resolve  legacy      a user sshd refuses: AUTH
#   app-internal behind a jump host            new-box     a key never seen before
#
# Screenshots land in docs/img/desktop-d2*.png (.vi.png: the Vietnamese UI);
# those of the steps in between, in target/smoke/.
#
# shortcut: the window is driven blind with xdotool, and what is asserted is
# what the app leaves behind (its window, its database, sshd's log, a live
# process), not the DOM; look at the screenshots for that. Upgrade to
# tauri-driver once WebKitWebDriver is installed (apt: webkit2gtk-driver).
set -eu
cd "$(dirname "$0")/.."
SIZE=1100x700
SSHD=/usr/sbin/sshd

if [ -z "${VSSH_SMOKE_DISPLAY:-}" ]; then
    for tool in xvfb-run xdotool ffmpeg npm ssh-keygen python3 "$SSHD"; do
        command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed"; exit 1; }
    done
    # `tauri build` puts the frontend inside the binary; a plain `cargo
    # build` makes one that looks for the dev server.
    (cd crates/vssh-desktop && npm run tauri build -- --debug --no-bundle)
    exec env VSSH_SMOKE_DISPLAY=1 xvfb-run -a -s "-screen 0 ${SIZE}x24" "$0"
fi

mkdir -p target/smoke
home=$(mktemp -d)
app=
sshd=
trap 'kill $app $sshd 2>/dev/null; rm -rf "$home"' EXIT
mkdir -p "$home/.ssh" "$home/.local/share" "$home/sshd"
chmod 700 "$home/.ssh"
# The user's own fonts (Maple Mono NF) are under the HOME this run hides.
[ -d "$HOME/.local/share/fonts" ] && ln -s "$HOME/.local/share/fonts" "$home/.local/share/fonts"

# --- the fixture hosts -------------------------------------------------------
ssh-keygen -q -t ed25519 -N '' -f "$home/.ssh/id_ed25519"
ssh-keygen -q -t ed25519 -N '' -f "$home/sshd/host_key"
cp "$home/.ssh/id_ed25519.pub" "$home/.ssh/authorized_keys"
cp scripts/smoke-hosts/*.txt "$home/sshd/"
# The fourth word of SSH_CONNECTION is the port the client came in on.
cat >"$home/sshd/answer" <<EOF
#!/bin/sh
set -- \$SSH_CONNECTION
cat "$home/sshd/\$4.txt"
EOF
chmod +x "$home/sshd/answer"
cat >"$home/sshd/config" <<EOF
ListenAddress 127.0.0.1:2201
ListenAddress 127.0.0.1:2202
ListenAddress 127.0.0.1:2203
ListenAddress 127.0.0.2:2201
HostKey $home/sshd/host_key
PidFile $home/sshd/pid
AuthorizedKeysFile $home/.ssh/authorized_keys
StrictModes no
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
ForceCommand $home/sshd/answer
LogLevel VERBOSE
EOF
"$SSHD" -D -e -f "$home/sshd/config" 2>"$home/sshd/log" &
sshd=$!

# Hosts the user "has connected to before": their key is on record. new-box
# (127.0.0.2) is left out on purpose.
key=$(cut -d' ' -f1,2 "$home/sshd/host_key.pub")
for name in '[127.0.0.1]:2201' '[127.0.0.1]:2202' '[127.0.0.1]:2203' '[127.0.0.1]:2209' db-replica.invalid; do
    echo "$name $key"
done >"$home/.ssh/known_hosts"

cat >"$home/.ssh/config" <<EOF
Host web-01
    HostName 127.0.0.1
    Port 2201
Host web-02
    HostName 127.0.0.1
    Port 2202
Host db-primary
    HostName 127.0.0.1
    Port 2203
Host db-replica
    HostName db-replica.invalid
    User postgres
Host cache-01
    HostName 127.0.0.1
    Port 2209
Host legacy
    HostName 127.0.0.1
    Port 2201
    User nobody
Host app-internal
    HostName 10.20.0.7
    User deploy
    ProxyJump web-01
Host new-box
    HostName 127.0.0.2
    Port 2201
EOF

shot() {
    sleep 1
    ffmpeg -loglevel error -y -f x11grab -draw_mouse 0 -video_size "$SIZE" -i "$DISPLAY" -frames:v 1 "$1"
    echo "shot: $1"
}
step() { echo "ok: $1"; }
accepted() { grep -c 'Accepted publickey' "$home/sshd/log" || true; }

# No GPU on a virtual display. No agent: only the throwaway key is offered.
env -u SSH_AUTH_SOCK HOME="$home" WEBKIT_DISABLE_DMABUF_RENDERER=1 target/debug/vssh-desktop &
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

# Groups and tags, written the way the TUI writes them, while the window is
# open: it has to notice on its own.
python3 - "$home/.local/share/vssh/vssh.db" <<'EOF'
import sqlite3, sys, time
rows = [
    ("web-01", "prod", "web eu", 1), ("web-02", "prod", "web us", 0),
    ("db-primary", "prod", "db eu", 0), ("db-replica", "prod", "db us", 0),
    ("cache-01", "staging", "cache", 0), ("legacy", "staging", "web", 0),
    ("app-internal", "staging", "app", 0), ("new-box", "", "", 0),
]
db = sqlite3.connect(sys.argv[1], timeout=10)
# The file is there before the app has put its hosts in it.
for _ in range(100):
    try:
        if db.execute("SELECT count(*) FROM hosts").fetchone()[0] == len(rows):
            break
    except sqlite3.OperationalError:
        pass
    time.sleep(0.1)
else:
    sys.exit("FAIL: the app never imported the hosts")
for alias, grp, tags, pinned in rows:
    db.execute("UPDATE hosts SET grp=?, tags=?, pinned=? WHERE alias=?", (grp, tags, pinned, alias))
db.commit()
EOF
step "groups and tags written to the database behind the window's back"

# Three rounds of the poll: long enough for every host to have answered, for
# the tags to have been read, and for a second login to show if there is one.
sleep 13
shot docs/img/desktop-d2.png

# One connection per host that lets us in, however many rounds were asked.
logins=$(accepted)
[ "$logins" -eq 3 ] || { echo "FAIL: $logins logins after three rounds, expected 3 (one per host)"; cat "$home/sshd/log"; exit 1; }
step "three rounds of metrics over three kept connections ($logins logins)"
grep -q '127.0.0.2' "$home/.ssh/known_hosts" && { echo "FAIL: the dashboard recorded new-box's key on its own"; exit 1; }
step "the host nobody connected to was left alone"

xdotool windowfocus "$win"
# / is the search box; three Tabs on is the first card.
first_card() { xdotool key --window "$win" slash Tab Tab Tab; }
# The language button, bottom right.
language() { xdotool mousemove --window "$win" 1073 686 click 1; }

# The quick peek, by its key: on the first card, which is being polled.
first_card
xdotool key --window "$win" v
shot docs/img/desktop-d2-peek.png
xdotool key --window "$win" Escape

language
sleep 6
shot docs/img/desktop-d2.vi.png
first_card
xdotool key --window "$win" v
shot docs/img/desktop-d2-peek.vi.png
xdotool key --window "$win" Escape
language

# And by resting the pointer on a card.
xdotool mousemove --window "$win" 880 310
sleep 1
shot target/smoke/hover.png
xdotool mousemove --window "$win" 600 20

# A person asking about the host nobody connected to: the last card, two tab
# stops per card further on. That is a decision: its key goes on record, as
# with `v` in the TUI, and from then on it is polled like the others.
first_card
xdotool key --window "$win" --repeat 14 Tab
xdotool key --window "$win" v
sleep 3
shot target/smoke/asked.png
xdotool key --window "$win" Escape
grep -q '127.0.0.2' "$home/.ssh/known_hosts" || { echo "FAIL: v on new-box did not ask it"; exit 1; }
sleep 6
logins=$(accepted)
[ "$logins" -eq 4 ] || { echo "FAIL: $logins logins, expected 4 once new-box is polled too"; exit 1; }
step "v asked the new host, and it joined the poll on one connection of its own"

# A card moved by hand: db-primary dragged onto web-01, its neighbour on the left.
xdotool mousemove --window "$win" 880 310 mousedown 1
for x in 860 800 700 600 500 470; do xdotool mousemove --window "$win" "$x" 312; sleep 0.1; done
xdotool mouseup 1
shot target/smoke/dragged.png

# The same without a mouse: Alt-Right moves the first card one place along.
first_card
xdotool key --window "$win" alt+Right
shot target/smoke/moved.png

xdotool key --window "$win" ctrl+k
xdotool type --window "$win" db
xdotool key --window "$win" Return

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the palette chose a host"
echo "gui smoke: ok (now look at the screenshots)"
