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
# Then the terminal tabs, on hosts added to the config while the window is
# open:
#
#   shell-01, shell-02  a real shell on a PTY  moved-box   a key that changed
#   far-box             its second jump host refuses
#
# Then the file view, on one more host:
#
#   files-01            an SFTP server over a directory of its own, fed at
#                       2 MiB a second so that a transfer can be looked at
#
# Then snippets, a batch and forwards, on fixtures made behind the window's
# back the way the TUI makes them:
#
#   mark                a snippet with a parameter, in the database
#   tunnel-01           web-01's sshd under another name, with two forwards in
#                       the config: one to files-01's port, one that wants a
#                       port sshd itself has
#
# In between, the desktop's keys (D10), where the real web view and the real
# xterm.js have to agree with the tests' jsdom:
#
#   Ctrl-Tab            and Ctrl-Shift-Tab, from inside a terminal: the tabs
#   the cheatsheet      by ?, by the settings' row, by Ctrl-/ and by F1
#   Ctrl-T              the palette: an address typed there is connected to as
#                       it stands; in a terminal Ctrl-T and ? are still the shell's
#   Ctrl-F              the hosts' search
#   Ctrl-Shift-D        a second shell beside the first; a click chooses the
#                       pane, a tab dragged to an edge moves it there
#   Ctrl-Shift-B        focus mode: no rail; Ctrl-B is still the shell's
#   a middle click      closes a tab
#
# And the pointer's ways and the rail (D11):
#
#   Ctrl-W, Ctrl-K      in a terminal they are the shell's: a word deleted, a
#                       line cut, and the tab still there; with Shift the window's
#   a busy tab          is asked about before it goes, and Enter is not a yes
#   the right button    on a card copies its address, in a terminal pastes
#   a tab               let go on another in the strip stands before it
#   the line            between two panes follows the pointer
#   the rail            narrow: its icons, a wider terminal, kept by the web view
#   what was open       is open again when the app comes back, after the vault
#
# And the hosts in the middle (D12):
#
#   the rail            names no host: where the list was it is empty
#   the search          Enter there is the card found; the arrow down too
#   the broadcast       button of the strip: one line in both shells; a pane's
#                       own switch takes it out and in again
#   Ctrl-Shift-V        pastes into the terminal in front, once
#   the other pane      keeps its cursor, and it breathes
#   Files in the rail   asks whose, and opens that host's
#
# Then the vault, the host form and the key setup (D6):
#
#   the vault           made by a passphrase, a sealed backup exported; at the
#                       end the app is started again and asks for it, once
#                       in each language
#   smoke-new           a host added through the form, renamed, deleted
#   bastion-x, inner-x  hosts of another ssh config, one of them imported
#   web-01              given a password in the vault, then the key setup: it
#                       makes a key and stops at the backup of sshd_config,
#                       which the forced command does not run. Nothing of this
#                       machine's sshd is touched.
#
# Then the rail (D9), with a session open, a host selected and the vault open:
#
#   Sessions            its row brings the open session's terminal to the front
#   the first row       the dashboard again, with its keys
#
# Then the files chosen in the desktop's dialog, the road through jump hosts,
# and the watching that can be stopped (D14):
#
#   gate-x              web-01's sshd again, a jump host whose own block names
#                       a key that does not open it
#   deep-x              shell-01's sshd behind it: refused at the jump host,
#                       until the form gives that hop a key of its own, chosen
#                       in the real GTK file dialog; then a shell through two
#                       hops. The config says ProxyJump a,b; the key is in the
#                       database
#   web-01              its card's Watch switch: the connection kept for it is
#                       closed and nobody logs in until it is on again; then
#                       the settings' Pause watching, for every host at once
#
# Then what D7 added, in the app as it was started last, in Vietnamese:
#
#   the bar             over the cards: with cards marked it is still two rows
#                       from one margin, the search and under it the tags
#   the Day theme       TokyoNight Day around a terminal that stays dark
#   streamer mode       made-up addresses on screen, the real ones in the
#                       config and on the wire; both switches are kept by the
#                       web view and are as they were when the app comes back
#
# And last the update (D15), in a build of this source that calls itself
# 0.0.1, over a HOME of its own:
#
#   the line            says the release GitHub has is out, with Update;
#                       nothing is fetched before the click
#   Update              with no network: an error and Retry. Then the real
#                       .deb from GitHub, read by dpkg-deb; dpkg's refusal,
#                       nobody to ask for the password, and installed: Restart
#                       ends the app and starts it again. pkexec, dpkg-query
#                       and xdg-open are stand-ins, so nothing is installed on
#                       this machine. Skipped, and said so, where GitHub does
#                       not answer
#
# Screenshots land in docs/img/desktop-d1*.png, desktop-d2*.png, desktop-d3*.png,
# desktop-d4*.png, desktop-d5*.png, desktop-d6*.png, desktop-d7*.png,
# desktop-d9-rail*.png, desktop-d10-{shortcuts,split,focus}*.png,
# desktop-d11-compact*.png and desktop-d12-broadcast*.png
# (.vi.png: the Vietnamese UI; d3-fail: a connection that fails, d3-closed:
# one that drops; d4-sftp: the file view with transfers under way; d5-batch:
# what four hosts answered to one command; d6-vault: the vault's manager,
# d6-host: the host form, as D14 leaves it: a key file and its folder button,
# a road of two hops; d7-day: the Day theme, d7-streamer: streamer mode;
# d9-rail: the rail with every row in use; d10-shortcuts: the cheatsheet,
# d10-split: two shells side by side, d10-focus: the same with no rail;
# d11-compact: the rail as icons; d12-broadcast: two panes with broadcast on);
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
    for tool in xvfb-run xdotool ffmpeg npm ssh-keygen ssh-keyscan python3 "$SSHD" /usr/lib/openssh/sftp-server; do
        command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed"; exit 1; }
    done
    # `tauri build` needs an inotify watcher; `npm run build` + plain
    # `cargo build --release` embeds the same frontendDist without one —
    # but only when the config has no devUrl, so stash it for the build.
    conf=crates/vssh-desktop/src-tauri/tauri.conf.json
    cp "$conf" "$conf.smoke-bak"
    trap 'mv "$conf.smoke-bak" "$conf"' EXIT
    python3 - "$conf" <<'PY'
import json, sys
p = sys.argv[1]
c = json.load(open(p))
c['build'].pop('devUrl', None)
c['build'].pop('beforeDevCommand', None)
json.dump(c, open(p, 'w'), indent=2)
PY
    (cd crates/vssh-desktop && npm run build && cd ../.. && cargo build --release -p vssh-desktop)
    # The same source once more, as a version long past, for the update (D15):
    # tauri merges TAURI_CONFIG over the config. A target directory of its own,
    # so that the build above is never the one that calls itself 0.0.1.
    TAURI_CONFIG='{"version":"0.0.1"}' cargo build --release -p vssh-desktop --target-dir target/smoke-old
    mv "$conf.smoke-bak" "$conf"; trap - EXIT
    exec env VSSH_SMOKE_DISPLAY=1 xvfb-run -a -s "-screen 0 ${SIZE}x24" "$0"
fi

mkdir -p target/smoke
home=$(mktemp -d)
app=
sshd=
# The app is given the time to go before its HOME is taken away: what it
# still writes on its way out would otherwise be in the way of the rm.
trap 'kill $app $sshd 2>/dev/null || true; wait 2>/dev/null || true; rm -rf "$home" 2>/dev/null || { sleep 2; rm -rf "$home"; }' EXIT
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
# shell-01 and shell-02 (2204, 2205) are real: a shell on the PTY, each in a
# directory of its own, a real `ps` for the question what runs in it, and a
# real run of what a batch sends them (the smoke's batches start with `touch`).
# Asked for their metrics they print web-01's; the rest only ever print.
ssh-keygen -q -t ed25519 -N '' -f "$home/sshd/old_key"
for port in 2204 2205; do
    mkdir "$home/sshd/$port"
    cp scripts/smoke-hosts/2201.txt "$home/sshd/$port.txt"
done
# files-01 (2206) is asked for its SFTP subsystem, which a forced command
# stands in for too: the real sftp-server, in a directory of its own, behind
# a pipe that lets 2 MiB a second through. Each time it is asked leaves a
# line in sshd/sftp-asked.
mkdir "$home/sshd/files"
cp scripts/smoke-hosts/2201.txt "$home/sshd/2206.txt"
cat >"$home/sshd/slow" <<'EOF'
import os, subprocess, time
server = subprocess.Popen(["/usr/lib/openssh/sftp-server"], stdin=subprocess.PIPE)
while data := os.read(0, 65536):
    server.stdin.write(data)
    server.stdin.flush()
    time.sleep(len(data) / (2 * 1024 * 1024))
server.stdin.close()
server.wait()
EOF
cat >"$home/sshd/answer" <<EOF
#!/bin/sh
set -- \$SSH_CONNECTION
case "\$4:\${SSH_ORIGINAL_COMMAND:-}" in
2206:*sftp-server*) echo >>"$home/sshd/sftp-asked"; cd "$home/sshd/files" && exec python3 "$home/sshd/slow" ;;
220[45]:) cd "$home/sshd/\$4" && exec env -i PATH=/usr/bin:/bin TERM="\$TERM" PS1="shell:\$4> " bash --norc --noprofile ;;
220[45]:*ps\ -o*) exec sh -c "\$SSH_ORIGINAL_COMMAND" ;;
220[45]:touch\ *) cd "$home/sshd/\$4" && exec sh -c "\$SSH_ORIGINAL_COMMAND" ;;
esac
cat "$home/sshd/\$4.txt"
EOF
chmod +x "$home/sshd/answer"
cat >"$home/sshd/config" <<EOF
ListenAddress 127.0.0.1:2201
ListenAddress 127.0.0.1:2202
ListenAddress 127.0.0.1:2203
ListenAddress 127.0.0.2:2201
ListenAddress 127.0.0.1:2204
ListenAddress 127.0.0.1:2205
ListenAddress 127.0.0.3:2201
ListenAddress 127.0.0.1:2206
Subsystem sftp /usr/lib/openssh/sftp-server
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
# The colour of one pixel of a screenshot, as rrggbb.
pixel() { ffmpeg -loglevel error -i "$1" -vf "crop=1:1:$2:$3" -f rawvideo -pix_fmt rgb24 - | od -An -tx1 | tr -d ' \n'; }

# No GPU on a virtual display. No agent: only the throwaway key is offered.
env -u SSH_AUTH_SOCK HOME="$home" WEBKIT_DISABLE_DMABUF_RENDERER=1 target/release/vssh-desktop &
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

# Concept A in TokyoNight Night is what is on screen: the page in the gutter
# left of the cards, the panel inside the first card (its padding, low enough
# to be under a down card's tint), and the band the search is on, right of
# its last button.
night="$(pixel docs/img/desktop-d2.png 264 300) $(pixel docs/img/desktop-d2.png 285 500) $(pixel docs/img/desktop-d2.png 1085 36)"
[ "$night" = "16161e 1f2335 1a1b26" ] || { echo "FAIL: page, card and the search's band are $night, not 16161e 1f2335 1a1b26"; exit 1; }
step "the Night theme: page #16161e, cards #1f2335, the search's band #1a1b26"

# D12: the rail names no host. Its seven rows end with the vault's, and from
# there down to the tags, where rows were pushed by the list of hosts, it is
# its own ground and nothing else: one colour along every line looked at.
for y in 370 400 430 460 490 520; do
    [ "$(ffmpeg -loglevel error -i docs/img/desktop-d2.png -vf "crop=150:1:54:$y" -f rawvideo -pix_fmt rgb24 - | od -An -v -tx1 -w3 | sort -u | wc -l)" -eq 1 ] ||
        { echo "FAIL: the rail has something at y $y, under its rows: a list of hosts?"; exit 1; }
done
step "the rail has no list of hosts: under its rows it is empty down to the tags"

# One connection per host that lets us in, however many rounds were asked.
logins=$(accepted)
[ "$logins" -eq 3 ] || { echo "FAIL: $logins logins after three rounds, expected 3 (one per host)"; cat "$home/sshd/log"; exit 1; }
step "three rounds of metrics over three kept connections ($logins logins)"
grep -q '127.0.0.2' "$home/.ssh/known_hosts" && { echo "FAIL: the dashboard recorded new-box's key on its own"; exit 1; }
step "the host nobody connected to was left alone"

xdotool windowfocus "$win"
# / is the search box; the arrow down from it is the first card, however many
# tags and controls are in the bar between them.
first_card() { xdotool key --window "$win" slash Down; }
# The dashboard back at its top: the wheel, in the gutter between the two
# columns of cards. A quick peek that opens under a card scrolls the page.
top() { xdotool mousemove --window "$win" 672 420 click --repeat 40 --delay 10 4; aside; }
# The pointer where it rests on nothing in any view: the status bar, right of
# its figures. No card's peek, no button's tooltip. (In focus mode, which has
# no status bar, that is the foot of a terminal.)
aside() { xdotool mousemove --window "$win" 700 690; sleep 0.5; }
# The settings, at the rail's foot: a menu whose first row has the keys. Down to
# the row asked for, and Enter: 1 the language, 2 the theme, 3 streamer mode,
# 4 pause watching, 5 focus mode, 6 the cheatsheet. RAIL: the middle of the rail, which may be narrow.
RAIL=120
setting() {
    xdotool mousemove --window "$win" "$RAIL" 636 click 1
    sleep 0.5
    n=$1
    while [ "$n" -gt 1 ]; do xdotool key --window "$win" Down; n=$((n - 1)); done
    xdotool key --window "$win" Return
    aside
}
language() { setting 1; }
# What the web view keeps of a setting: its localStorage, a database of WebKit's.
kept() {
    sleep 3
    python3 - "$home/.local/share/io.github.duclong06.vssh/localstorage/tauri_localhost_0.localstorage" "$1" <<'PY'
import sqlite3, sys
row = sqlite3.connect(sys.argv[1], timeout=10).execute("SELECT value FROM ItemTable WHERE key=?", (sys.argv[2],)).fetchone()
print(row[0].decode("utf-16-le") if row else "none")
PY
}

# The quick peek, by its key: on the first card, which is being polled.
first_card
xdotool key --window "$win" v
shot docs/img/desktop-d2-peek.png
xdotool key --window "$win" Escape
top

language
sleep 6
shot docs/img/desktop-d2.vi.png
first_card
xdotool key --window "$win" v
shot docs/img/desktop-d2-peek.vi.png
xdotool key --window "$win" Escape
language
top

# And by resting the pointer on a card: db-replica, which is down.
xdotool mousemove --window "$win" 875 440
sleep 1
shot target/smoke/hover.png
aside

# A person asking about the host nobody connected to: the last card, which
# the palette finds. That is
# a decision: its key goes on record, as with `v` in the TUI, and from then
# on it is polled like the others.
xdotool key --window "$win" ctrl+k
xdotool type --window "$win" new-box
xdotool key --window "$win" Return
sleep 1
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
top
xdotool mousemove --window "$win" 880 605 mousedown 1
for x in 860 800 700 600 500 470; do xdotool mousemove --window "$win" "$x" 607; sleep 0.1; done
xdotool mouseup 1
shot target/smoke/dragged.png
# The order somebody made is the web view's to keep: db-primary leads it now.
case "$(kept vssh.order)" in
'["db-primary","web-01",'*) ;;
*) echo "FAIL: a card dragged onto its neighbour did not take its place: the order kept is $(kept vssh.order)"; exit 1 ;;
esac
step "a card dragged onto its neighbour stands before it, and the web view kept the order"

# The same without a mouse: Alt-Right moves the first card one place along.
first_card
xdotool key --window "$win" alt+Right
shot target/smoke/moved.png

# The three pictures of D1, as the window is now: at rest, the palette with
# what `db` finds, and the host it chose as the selected one.
top
shot docs/img/desktop-d1.png
xdotool key --window "$win" ctrl+k
xdotool type --window "$win" db
shot docs/img/desktop-d1-palette.png
xdotool key --window "$win" Return
aside
shot docs/img/desktop-d1-selected.png

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the palette chose a host"

# --- D3: terminal tabs -------------------------------------------------------
# Four more hosts, while the window is open. moved-box is on record with a
# key that is not the one it has now.
cat >>"$home/.ssh/config" <<EOF
Host shell-01
    HostName 127.0.0.1
    Port 2204
Host shell-02
    HostName 127.0.0.1
    Port 2205
Host moved-box
    HostName 127.0.0.3
    Port 2201
Host far-box
    HostName 10.20.0.9
    ProxyJump web-01,cache-01
EOF
old=$(cut -d' ' -f1,2 "$home/sshd/old_key.pub")
echo "[127.0.0.3]:2201 $old" >>"$home/.ssh/known_hosts"
sleep 6

key() { xdotool key --window "$win" "$@"; sleep 0.3; }
# What a person types at the terminal in front, and Enter.
run() { xdotool type --window "$win" --delay 20 "$1"; key Return; sleep 1; }
# The palette finds the host's card; Enter on the card opens its tab. With
# Shift the palette's key is the window's in a terminal too.
open_tab() {
    key ctrl+shift+k
    xdotool type --window "$win" "$1"
    key Return
    key Return
    sleep 3
}
there() { [ -e "$home/sshd/$1" ] || { echo "FAIL: $2"; shot target/smoke/failed.png; exit 1; }; }

open_tab shell-01
run 'echo hello from a real pty'
run 'touch typed'
there 2204/typed "what was typed in the tab did not run in shell-01's shell"
run 'stty size >size'
there 2204/size "stty did not run"
size=$(cat "$home/sshd/2204/size")
[ "$size" != "24 80" ] || { echo "FAIL: the PTY is 24x80, not the size of the terminal"; exit 1; }
step "a tab is a real shell on a PTY the size of its terminal ($size)"

open_tab shell-02
run 'touch typed'
there 2205/typed "the second tab is not a shell of its own"
[ ! -e "$home/sshd/2204/both" ] || { echo "FAIL: shell-01 heard what was typed in shell-02"; exit 1; }
step "a second tab is a second shell; without broadcast each hears only itself"

# The desktop's keys for the tabs, which no shell is sent: Ctrl-Shift-Tab is
# the tab before, Ctrl-Tab the one after. What is typed then says where it went.
key ctrl+shift+Tab
run 'touch stepped-back'
there 2204/stepped-back "Ctrl-Shift-Tab did not bring shell-01's tab to the front"
key ctrl+Tab
run 'touch stepped-on'
there 2205/stepped-on "Ctrl-Tab did not bring shell-02's tab back to the front"
step "Ctrl-Shift-Tab and Ctrl-Tab went through the tabs, from inside a terminal"

# D12: the strip's own switch, there since two shells are open. One click,
# and one line typed runs in both: the click left the keys where they were.
# The pointer goes back into the terminal: no tooltip in a picture.
cast() { xdotool mousemove --window "$win" 960 24 click 1; xdotool mousemove --window "$win" 700 400; sleep 0.7; }
# A pane's own switch, top right of its terminal while broadcast is on.
PANE_CAST="1034 75"
cast
run 'touch cast'
there 2204/cast "the strip's broadcast button: what was typed did not reach shell-01"
there 2205/cast "the strip's broadcast button: what was typed did not run in shell-02, where it was typed"
shot target/smoke/cast-on.png
# shell-02, in front, out of it by its own switch: it hears itself only, and
# is not heard.
xdotool mousemove --window "$win" $PANE_CAST click 1
xdotool mousemove --window "$win" 700 400
sleep 0.7
run 'touch alone'
shot target/smoke/cast-pane-off.png
there 2205/alone "a pane switched out of broadcast lost the keys"
[ ! -e "$home/sshd/2204/alone" ] || { echo "FAIL: shell-01 heard a pane that was switched out of broadcast"; exit 1; }
# In again by the same switch; then all of it off by the strip's.
xdotool mousemove --window "$win" $PANE_CAST click 1
xdotool mousemove --window "$win" 700 400
sleep 0.7
run 'touch together'
there 2204/together "a pane switched back into broadcast was not heard by the other"
cast
run 'touch after-cast'
there 2205/after-cast "with broadcast off again the terminal in front lost the keys"
[ ! -e "$home/sshd/2204/after-cast" ] || { echo "FAIL: the strip's button did not turn broadcast off"; exit 1; }
step "the strip's broadcast button: one line ran in both shells; a pane's own switch took it out and in; off again by the button"

# Broadcast on both tabs: one line typed, two shells run it.
key ctrl+a b
key ctrl+a 1
key ctrl+a b
run 'touch both'
there 2204/both "broadcast did not reach shell-01"
there 2205/both "broadcast did not reach shell-02"
step "broadcast: one line typed in tab 1 ran in both shells"

# Ctrl-A y: the output of the last command is on the clipboard.
key ctrl+a b
run 'echo copied by the leader'
key ctrl+a y
sleep 1
if command -v xclip >/dev/null; then
    copied=$(xclip -o -selection clipboard)
    [ "$copied" = "copied by the leader" ] || { echo "FAIL: Ctrl-A y copied '$copied'"; exit 1; }
    step "Ctrl-A y put the last command's output on the clipboard"
fi
key ctrl+a b

# Something that takes a while, in both: the tabs say what runs.
run 'uname -sr'
run 'sleep 60'
sleep 5
shot docs/img/desktop-d3-terminal.png
# The language button takes the keyboard; a click gives it back to the
# terminal, so both shots are of the same state.
terminal() { xdotool mousemove --window "$win" 700 400 click 1; }
language
terminal
shot docs/img/desktop-d3-terminal.vi.png
language
# The Night theme again, in the terminal itself.
night=$(pixel docs/img/desktop-d3-terminal.png 900 500)
[ "$night" = "1a1b26" ] || { echo "FAIL: the terminal's background is $night, not 1a1b26"; exit 1; }
step "the terminal is the Night theme too"
# Both tabs broadcast: the pill in front has the full yellow border along its
# top, the one behind a dimmer one, and neither the strip's own colour. The
# strip is the top row of the window.
front=$(pixel docs/img/desktop-d3-terminal.png 450 8)
behind=$(pixel docs/img/desktop-d3-terminal.png 700 8)
[ "$front" = e0af68 ] && [ "$behind" != "$front" ] && [ "$behind" != 1f2335 ] ||
    { echo "FAIL: broadcasting tabs' borders are $front in front and $behind behind"; exit 1; }
step "while broadcasting, the tab in front is told apart (#$front, behind #$behind)"
# Back into the terminal, and Ctrl-C for both sleeps.
terminal
key ctrl+c
key ctrl+a B
key ctrl+a B

# A key that changed: nobody is let in before a person answers.
logins=$(accepted)
open_tab moved-box
shot target/smoke/hostkey.png
[ "$(accepted)" -eq "$logins" ] || { echo "FAIL: logged in to moved-box before the answer"; exit 1; }
# Cancel has the focus; Accept once is next.
key Tab
key Return
sleep 3
shot target/smoke/accepted-once.png
[ "$(accepted)" -eq $((logins + 1)) ] || { echo "FAIL: Accept once did not connect"; cat "$home/sshd/log"; exit 1; }
[ "$(grep -F '[127.0.0.3]:2201' "$home/.ssh/known_hosts" | cut -d' ' -f2,3)" = "$old" ] || { echo "FAIL: Accept once wrote to known_hosts"; exit 1; }
step "a changed host key: nothing before the answer; Accept once connects and writes nothing"
# Once was once. Trust puts the key the host has now on record. In a terminal
# the window's key to close a tab is the one with Shift.
key ctrl+shift+w
open_tab moved-box
key Tab
key Tab
key Return
sleep 3
[ "$(grep -F '[127.0.0.3]:2201' "$home/.ssh/known_hosts" | cut -d' ' -f2,3)" = "$key" ] || { echo "FAIL: Trust did not replace the key on record"; cat "$home/.ssh/known_hosts"; exit 1; }
step "Trust: the key on record is the one the host has now"
key ctrl+shift+w

# A jump host that is down, second of two: the first is logged in to, and
# the dialog says where it stopped. moved-box is trusted now and joins the
# poll with a login of its own: a round goes by first, so that it is counted
# before and not as far-box's.
sleep 6
logins=$(accepted)
open_tab far-box
shot docs/img/desktop-d3-fail.png
[ "$(accepted)" -eq $((logins + 1)) ] || { echo "FAIL: far-box's first jump host was not logged in to ($(accepted) logins, $logins before)"; exit 1; }
step "a jump chain that breaks at hop 2 got through hop 1"
key Escape
# The same in Vietnamese. The dialog is modal: the language goes first.
key ctrl+shift+w
language
open_tab far-box
shot docs/img/desktop-d3-fail.vi.png
key Escape
key ctrl+shift+w
language

# A session that ends under its tab: shell-01's shell kills what it runs
# under. The tab keeps its screen and says so in a banner.
key ctrl+a B
key ctrl+a 1
run 'kill -9 $PPID'
sleep 2
shot docs/img/desktop-d3-closed.png
language
terminal
shot docs/img/desktop-d3-closed.vi.png
language
# The banner's red border, top right of the terminal.
red=$(pixel docs/img/desktop-d3-closed.png 992 61)
[ "$red" = f7768e ] || { echo "FAIL: no red banner on the closed tab (#$red)"; exit 1; }
step "a session that ended leaves a red banner on its tab"

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the terminal tabs"

# --- D4: files ---------------------------------------------------------------
# What is on this machine (the throwaway HOME) and what is on the host. The
# two big files are holes: they take no room here, and at 2 MiB a second
# little of them gets there.
files="$home/sshd/files"
mkdir "$home/Documents" "$home/Downloads" "$files/releases" "$files/shared"
echo 'a note from this machine' >"$home/notes.txt"
echo 'hello from this machine' >"$home/hello.txt"
truncate -s 96M "$home/site.tar.gz"
truncate -s 64M "$home/backup.sql"
echo 'deployed by the fixture' >"$files/deploy.log"
echo 'worker_processes auto;' >"$files/nginx.conf"
cat >>"$home/.ssh/config" <<EOF
Host files-01
    HostName 127.0.0.1
    Port 2206
EOF
sleep 6

fail() { echo "FAIL: $1"; shot target/smoke/failed.png; exit 1; }
sftps() { wc -l <"$home/sshd/sftp-asked" 2>/dev/null || echo 0; }
size() { stat -c %s "$1" 2>/dev/null || echo 0; }
# The palette finds the host's card; f on the card opens its files.
open_files() {
    key ctrl+shift+k
    xdotool type --window "$win" "$1"
    key Return
    key f
    sleep 4
}
# The rows of the two panes: 23 px each, the middle of the first at y 133.
row() { echo $((133 + 23 * $1)); }
LOCAL=420 REMOTE=860
click_row() { xdotool mousemove --window "$win" "$1" "$(row "$2")" click "${3:-1}"; sleep 0.5; }
# A row dragged from one pane into the other.
drag_row() {
    xdotool mousemove --window "$win" "$1" "$(row "$2")" mousedown 1
    for part in 1 2 3 4 5 6; do
        xdotool mousemove --window "$win" $(($1 + ($3 - $1) * part / 6)) 420
        sleep 0.1
    done
    xdotool mouseup 1
    sleep 3
}

open_files files-01
shot target/smoke/files.png
[ "$(sftps)" -ge 2 ] || fail "the file view did not ask files-01 over SFTP ($(sftps) requests)"
step "f on a card opened its files: this machine left, the host right"

# The keys are the local pane's: the last file but one is notes.txt.
key G k c
sleep 3
cmp -s "$home/notes.txt" "$files/notes.txt" || fail "c did not copy notes.txt to the host"
step "c copied the selected file up to the host, byte for byte"

# The right button under the host's rows: the pane's own menu. Upload, for
# what this machine's pane has chosen, then New folder.
xdotool mousemove --window "$win" "$REMOTE" 540 click 3
sleep 0.5
shot target/smoke/files-menu.png
key Down Return
xdotool type --window "$win" made-here
key Return
sleep 3
[ -d "$files/made-here" ] || fail "the menu's New folder made nothing on the host"
step "the right button's menu made a folder on the host"

# The host's rows now: made-here, releases, shared, deploy.log, nginx.conf,
# notes.txt. On a file the menu is Download, New folder, Rename, Delete, and
# under a line Refresh.
click_row "$REMOTE" 3 3
key Down Down Return
xdotool type --window "$win" deploy.old
key Return
sleep 3
[ -e "$files/deploy.old" ] && [ ! -e "$files/deploy.log" ] || fail "Rename did not rename deploy.log"
# Delete asks first: Cancel has the focus, Delete is next.
click_row "$REMOTE" 4 3
key Down Down Down Return
sleep 0.5
key Tab Return
sleep 3
[ ! -e "$files/nginx.conf" ] || fail "Delete did not remove nginx.conf"
step "the menu renamed one file on the host and, after a yes, deleted another"

# Drag and drop, both ways. Here: Documents, Downloads, sshd, backup.sql,
# hello.txt, notes.txt, site.tar.gz. There: ..., deploy.old, notes.txt.
drag_row "$LOCAL" 4 "$REMOTE"
cmp -s "$home/hello.txt" "$files/hello.txt" || fail "a row dragged to the host's pane was not uploaded"
drag_row "$REMOTE" 3 "$LOCAL"
cmp -s "$files/deploy.old" "$home/deploy.old" || fail "a row dragged from the host's pane was not downloaded"
step "a row dragged across was copied: one up, one down"

# The two big files, marked and sent together. Here now: ..., backup.sql
# (3), deploy.old, hello.txt, notes.txt, site.tar.gz (last).
click_row "$LOCAL" 3
key space G space c
sleep 5
# The pointer off the rows: no hover band next to the selected one.
click_row "$LOCAL" 7
shot docs/img/desktop-d4-sftp.png
# The language button takes the keyboard; a click on the row that is
# selected gives it back to the pane, so both shots are of the same state.
language
click_row "$LOCAL" 7
shot docs/img/desktop-d4-sftp.vi.png
language
for name in backup.sql site.tar.gz; do
    [ "$(size "$files/$name")" -gt 0 ] || fail "$name is not on its way to the host"
done
[ "$(size "$files/site.tar.gz")" -lt "$(size "$home/site.tar.gz")" ] || fail "the transfer was over before it could be looked at"
step "two transfers at once, each with its line ($(size "$files/backup.sql") and $(size "$files/site.tar.gz") bytes so far)"

# Cancel on the first line stops that one where it is; the other goes on.
xdotool mousemove --window "$win" 1057 595 click 1
sleep 2
stopped=$(size "$files/backup.sql")
going=$(size "$files/site.tar.gz")
sleep 3
[ "$(size "$files/backup.sql")" -eq "$stopped" ] || fail "backup.sql still grows after Cancel"
[ "$(size "$files/site.tar.gz")" -gt "$going" ] || fail "Cancel stopped the other transfer too"
shot target/smoke/files-cancelled.png
step "Cancel stopped one transfer where it was ($stopped bytes), the other went on"
# Closing the tab stops what it was moving.
key ctrl+w
sleep 2
going=$(size "$files/site.tar.gz")
sleep 3
[ "$(size "$files/site.tar.gz")" -eq "$going" ] || fail "site.tar.gz still grows after its tab was closed"
step "closing the tab stopped the transfer that was left"

# A host that is down: its pane says how, this machine's pane works.
open_files cache-01
shot target/smoke/files-dead.png
key ctrl+w

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the file views"

# --- D5: snippets, a batch, forwards -----------------------------------------
# The fixtures, while the window is open. The two shells get a group each:
# a host's name in a batch has its group's colour.
python3 - "$home/.local/share/vssh/vssh.db" <<'EOF'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1], timeout=10)
command = "touch snippet-{{name}} && echo {{name}} ran in $PWD"
db.execute("INSERT INTO snippets(host, name, command) VALUES ('', 'mark', ?)", (command,))
for alias, grp, tags in [("shell-01", "dev", "app eu"), ("shell-02", "test", "app us")]:
    db.execute("UPDATE hosts SET grp=?, tags=? WHERE alias=?", (grp, tags, alias))
db.commit()
EOF
cat >>"$home/.ssh/config" <<EOF
Host tunnel-01
    HostName 127.0.0.1
    Port 2201
    LocalForward 18022 127.0.0.1:2206
    LocalForward 2206 127.0.0.1:2201
EOF
sleep 6

# The palette finds a host's card; Space marks it. Two that answer, one that
# refuses, one that lets nobody in.
mark() {
    key ctrl+shift+k
    xdotool type --window "$win" "$1"
    key Return
    key space
}
for name in shell-01 shell-02 cache-01 legacy; do mark "$name"; done
aside

# s: the snippets, with the one the database got meanwhile. Add has the
# focus, Run is the next stop; its form asks for the one parameter, and Enter
# there runs it on the four marked hosts.
key s
sleep 1
shot target/smoke/snippets.png
key Tab Return
xdotool type --window "$win" smoke
shot target/smoke/snippet-run.png
key Return
sleep 6
shot target/smoke/snippet-ran.png
there 2204/snippet-smoke "the snippet did not run on shell-01 with its parameter filled in"
there 2205/snippet-smoke "the snippet did not run on shell-02 with its parameter filled in"
step "a snippet from the database ran on the marked hosts, {{name}} filled in from its form"
key Escape

# x: one command typed for the same four. They are still marked.
batch() {
    key x
    xdotool type --window "$win" --delay 20 'touch x-ran; pwd; uptime'
    key Return
    sleep 6
}
batch
there 2204/x-ran "the command typed after x did not run on shell-01"
there 2205/x-ran "the command typed after x did not run on shell-02"
shot docs/img/desktop-d5-batch.png
step "x ran one typed command on the marked hosts"
# y: what every host answered is on the clipboard, each under its name.
key y
sleep 1
if command -v xclip >/dev/null; then
    copied=$(xclip -o -selection clipboard)
    case "$copied" in
    *"# cache-01"*"# shell-01"*"$home/sshd/2204"*"# shell-02"*"$home/sshd/2205"*) ;;
    *) fail "y copied '$copied'" ;;
    esac
    step "y put every host's answer on the clipboard, the failed ones too"
fi
key slash
xdotool type --window "$win" shell-02
shot target/smoke/batch-filter.png
key Return
key Escape
# The same in Vietnamese. The dialog is modal: the language goes first.
language
batch
shot docs/img/desktop-d5-batch.vi.png
key Escape
language

# L on a card: its forwards. The first switch has the focus.
forwards() {
    key ctrl+shift+k
    xdotool type --window "$win" "$1"
    key Return
    key L
    sleep 2
}
# The host key that answers on a port of this machine.
through() { ssh-keyscan -T 5 -t ed25519 -p "$1" 127.0.0.1 2>/dev/null | cut -d' ' -f2,3; }
forwards tunnel-01
shot target/smoke/forwards.png
[ -z "$(through 18022)" ] || fail "port 18022 answers before its forward is switched on"
key Return
sleep 4
[ "$(through 18022)" = "$key" ] || fail "nothing answers through the forward on 18022"
shot target/smoke/forwards-on.png
step "a forward of the config, switched on, leads through its host to the port it names"

# The second wants 2206, which sshd has: the system says so, and the form
# offers the next port. That is the last stop: x, kind, port, where to, Add.
key Tab Tab Return
sleep 3
shot target/smoke/forwards-taken.png
key Tab Tab Tab Tab Tab Tab Return
# The port has the focus, with 2207 in it: Enter adds the forward.
key Return
sleep 2
grep -q 'LocalForward 2207 127.0.0.1:2201' "$home/.ssh/config" || fail "the forward on the port offered was not added to the config"
# Back from the port: the kind, the new row's x, its switch.
key shift+Tab shift+Tab shift+Tab Return
sleep 4
[ "$(through 2207)" = "$key" ] || fail "nothing answers through the forward added on 2207"
shot target/smoke/forwards-added.png
step "a port that is taken said so; the port offered was added and leads where the other would"

# Delete on the first: back over the second row. It is stopped and gone.
key shift+Tab shift+Tab shift+Tab Return
sleep 2
grep -q 'LocalForward 18022' "$home/.ssh/config" && fail "Delete left the forward in the config"
[ -z "$(through 18022)" ] || fail "port 18022 still answers after its forward was deleted"
step "Delete stopped a running forward and took it out of the config"
key Escape

# The same two dialogs in Vietnamese, to look at.
language
forwards tunnel-01
shot target/smoke/forwards.vi.png
key Escape
key s
sleep 1
shot target/smoke/snippets.vi.png
key Escape
language

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the snippets, the batches and the forwards"

# --- D6: the vault, the host form, the key setup ------------------------------
# An action of the palette by its English name, whatever the language is; or
# a host, whose card then has the focus.
action() {
    key ctrl+shift+k
    xdotool type --window "$win" "$1"
    key Return
    sleep 1
}
typed() { xdotool type --window "$win" --delay 20 "$1"; }
config="$home/.ssh/config"
# What the database has of a host: group, tags, pin.
meta() {
    python3 - "$home/.local/share/vssh/vssh.db" "$1" <<'EOF'
import sqlite3, sys
row = sqlite3.connect(sys.argv[1], timeout=10).execute("SELECT grp, tags, pinned FROM hosts WHERE alias=?", (sys.argv[2],)).fetchone()
print("|".join(map(str, row)) if row else "none")
EOF
}

# There is no vault yet: the first passphrase makes it, typed twice the same.
vault="$home/.local/share/vssh/vault"
[ ! -e "$vault" ] || fail "there is a vault before anyone made one"
action vault
shot target/smoke/vault-none.png
typed 'smoke master'
key Tab
typed 'smoke mastre'
key Return
sleep 1
[ ! -e "$vault" ] || fail "a vault was made from two passphrases that differ"
shot target/smoke/vault-mismatch.png
# The second one again. Over a dialog Ctrl-A is the field's own: select all.
key ctrl+a
typed 'smoke master'
key Return
sleep 2
[ "$(stat -c %a "$vault" 2>/dev/null)" = 600 ] || fail "the vault was not made, or not for its owner alone"
step "the vault was made by a passphrase typed twice the same, for its owner alone"
# The manager, with the focus in its first field: the two new passphrases,
# Change, the path, Export.
key Tab Tab Tab Tab Tab Return
sleep 2
[ "$(head -c 5 "$home/vssh-vault.backup" 2>/dev/null)" = VSSH1 ] || fail "no sealed backup at $home/vssh-vault.backup"
[ "$(stat -c %a "$home/vssh-vault.backup")" = 600 ] || fail "the backup can be read by others"
shot docs/img/desktop-d6-vault.png
step "a sealed backup of the vault was exported to the home directory"
key Escape
language
action vault
shot docs/img/desktop-d6-vault.vi.png
key Escape
language

# The host form. Sent empty it says what is wrong under each field and adds
# nothing; the alias has the focus again.
action 'add host'
key Return
shot target/smoke/host-invalid.png
# Alias, host, no user, port; the agent; no jump host; two tags, a group, the
# pin, notes.
typed smoke-new
key Tab
typed 127.0.0.1
key Tab Tab
typed 2204
key Tab Tab Tab
typed 'smoke eu '
key Tab
typed staging
key Tab space Tab
typed 'added by the smoke run'
shot target/smoke/host-new.png
# Import, Cancel, Save.
key Tab Tab Tab Return
sleep 2
block=$(grep -A2 '^Host smoke-new$' "$config" | tr -s ' \n' ' ')
[ "$block" = "Host smoke-new HostName 127.0.0.1 Port 2204 " ] || fail "the form's host is in the config as '$block'"
[ "$(meta smoke-new)" = "staging|smoke eu|1" ] || fail "the form's group, tags and pin are in the database as '$(meta smoke-new)'"
step "a host added through the form is in the config and the database as it was typed"
sleep 6
shot target/smoke/host-added.png

# Its card has the focus. e: its form, with what it has; another alias renames it.
key e
sleep 1
shot target/smoke/host-edit.png
key ctrl+a
typed smoke-box
key Return
sleep 2
grep -q '^Host smoke-new$' "$config" && fail "the renamed host is in the config under its old name too"
grep -q '^Host smoke-box$' "$config" || fail "the renamed host is not in the config"
[ "$(meta smoke-box)" = "staging|smoke eu|1" ] || fail "the renamed host lost its group, tags or pin: '$(meta smoke-box)'"
step "e opened the host's form; a new alias renamed it with everything it has"

# d asks first, and the question opens on Cancel: Enter is not a yes.
action smoke-box
key d
shot target/smoke/host-delete.png
key Return
sleep 1
grep -q '^Host smoke-box$' "$config" || fail "Enter on the question deleted the host"
action smoke-box
key d Tab Return
sleep 2
grep -q '^Host smoke-box$' "$config" && fail "Delete left the host in the config"
[ "$(meta smoke-box)" = none ] || fail "Delete left the host in the database"
step "d asked first: Enter kept the host, Delete took it out of the config and the database"

# Another ssh config: two hosts we have not, one we have. The form's own
# config has nothing new; of the other file's two, one is left ticked.
cat >"$home/their_config" <<EOF
Host bastion-x
    HostName 127.0.0.1
    Port 2201
Host inner-x
    HostName 10.30.0.9
    ProxyJump bastion-x
Host web-01
    HostName elsewhere.invalid
EOF
action 'add host'
key Tab Tab Tab Tab Tab Tab Tab Tab Tab Tab Return
sleep 2
shot target/smoke/import-none.png
typed '~/their_config'
key Return
sleep 2
# The folder button, Preview, bastion-x, inner-x (unticked), Cancel, Import.
key Tab Tab Tab Tab space
shot target/smoke/import-preview.png
key Tab Tab Return
sleep 2
grep -q '^Host bastion-x$' "$config" || fail "the host left ticked was not imported"
grep -q '^Host inner-x$' "$config" && fail "the host that was unticked was imported"
grep -q 'elsewhere.invalid' "$config" && fail "the import wrote over a host that was there"
step "an import took the one host left ticked, and left the host we had as it was"

language
action web-01
key e
sleep 1
shot target/smoke/host-edit.vi.png
key Escape
language

# web-01 gets a password in the vault: alias, host, user, port, then how it is
# signed in to, where the vault's password is two to the left of the agent.
action web-01
key e
sleep 1
key Tab Tab Tab Tab Left Left Tab
typed 'not-a-password'
key Return
sleep 2
grep -q 'not-a-password' "$config" "$home/.local/share/vssh/vssh.db" && fail "the password is outside the vault"
grep -q 'not-a-password' "$vault" && fail "the password is in the vault in the clear"
# Now its form offers the key setup: the last stop but two (Cancel, Save).
# On the way: host, user, port, the sign-in, the password, the jump host, the
# x of each of its two tags, the tag field, group, pin, notes, Delete.
action web-01
key e
sleep 1
shot target/smoke/host-password.png
key Tab Tab Tab Tab Tab Tab Tab Tab Tab Tab Tab Tab Tab Tab Return
sleep 1
shot target/smoke/key-setup.png
# Start has the focus. The key is made; the backup of sshd_config is a command
# this sshd does not run, so the setup stops there, with nothing to restore.
key Return
sleep 6
shot target/smoke/key-setup-failed.png
[ -f "$home/.ssh/vssh_web-01_ed25519" ] && [ -f "$home/.ssh/vssh_web-01_ed25519.pub" ] || fail "the key setup made no key"
grep -q 'vssh_web-01_ed25519' "$config" && fail "a key that was never proven is named in the config"
step "the key setup made a key, stopped at a host it could not change, and named no key in the config"
key Escape

# --- D9: the rail ------------------------------------------------------------
# The four hosts of the batches are unmarked again, and shell-02 is the
# selected one. Its session is the last one open: the rail's Sessions row
# brings it to the front, and what is typed then is typed in its shell.
for name in shell-01 cache-01 legacy shell-02; do mark "$name"; done
rail() { xdotool mousemove --window "$win" 120 "$1" click 1; sleep 1; }
rail 198
run 'touch rail'
there 2205/rail "the rail's Sessions row did not bring shell-02's terminal to the front"
step "the rail's Sessions row brought the open session to the front"
# Its first row is the dashboard again: there x is a key, and asks for a
# command for the selected host.
rail 129
key x
xdotool type --window "$win" --delay 20 'touch rail-back'
key Return
sleep 5
there 2205/rail-back "the rail's first row did not bring the dashboard back: x asked for no command"
key Escape
step "the rail's first row brought the dashboard back, with its keys"
# D12: files and forwards are some host's, and the rail names none: it asks
# whose. Left unanswered nobody is asked; answered with files-01, that host is
# asked for its SFTP. Ctrl-W closes the files' tab.
before=$(sftps)
rail 282
shot target/smoke/whose-files.png
key Escape
sleep 2
[ "$(sftps)" -eq "$before" ] || fail "the rail's Files row opened somebody's files before it was said whose"
rail 282
typed files-01
key Return
sleep 5
shot target/smoke/whose-files-answered.png
[ "$(sftps)" -gt "$before" ] || fail "the rail's Files row, answered with files-01, did not ask that host for its files"
key ctrl+w
key ctrl+a 0
step "the rail's Files row asked whose, and opened the files of the host it was told"
top
shot docs/img/desktop-d9-rail.png
language
top
shot docs/img/desktop-d9-rail.vi.png
language

# --- D14: a key chosen as a file, the road through jump hosts, the watching ---
# What the database has, as rows of |-separated fields.
rows() {
    python3 - "$home/.local/share/vssh/vssh.db" "$1" <<'PY'
import sqlite3, sys
for row in sqlite3.connect(sys.argv[1], timeout=10).execute(sys.argv[2]):
    print("|".join(map(str, row)))
PY
}
# The desktop's own file dialog: a window of its own, which nobody gives the
# keys on a display without a window manager. Ctrl-L is its field for a path.
choose() {
    dlg=$(timeout 10 xdotool search --sync --onlyvisible --name '^Choose a key file$' | head -n 1)
    [ -n "$dlg" ] || fail "the folder button opened no file dialog"
    xdotool windowfocus "$dlg"
    sleep 0.5
    xdotool key --window "$dlg" ctrl+l
    sleep 0.5
    xdotool type --window "$dlg" --delay 20 "$1"
    sleep 0.5
    shot target/smoke/pick-key.png
    xdotool key --window "$dlg" Return
    sleep 2
    xdotool search --onlyvisible --name '^Choose a key file$' >/dev/null && fail "the file dialog did not take the path typed in it"
    xdotool windowfocus "$win"
    sleep 0.5
}
# gate-x is web-01's sshd under a name whose block has a key that sshd does
# not know; deep-x is shell-01's.
ssh-keygen -q -t ed25519 -N '' -f "$home/.ssh/not_it"
cat >>"$config" <<EOF
Host gate-x
    HostName 127.0.0.1
    Port 2201
    IdentityFile ~/.ssh/not_it
Host deep-x
    HostName 127.0.0.1
    Port 2204
EOF
sleep 7

# The road, one hop long, and nothing said of a key: alias, host, user, port,
# how it is signed in to, and the road's one stop, which adds a row to type in.
action deep-x
key e
sleep 1
key Tab Tab Tab Tab Tab Return
typed gate-x
key Return
sleep 2
block=$(grep -A3 '^Host deep-x$' "$config" | tr -s ' \n' ' ')
[ "$block" = "Host deep-x HostName 127.0.0.1 Port 2204 ProxyJump gate-x " ] || fail "the road typed in the form is in the config as '$block'"
[ -z "$(rows "SELECT * FROM jump_auth")" ] || fail "a hop was given a key nobody chose"
# The jump host is logged in to with what its own block says, which does not
# open it: nobody gets in, and the tab says where it stopped.
sleep 6
logins=$(accepted)
open_tab deep-x
shot target/smoke/hop-refused.png
[ "$(accepted)" -eq "$logins" ] || fail "the jump host let its own key in: $(accepted) logins, $logins before"
key Escape
key ctrl+shift+w
step "a road typed in the form is ProxyJump in the config; the jump host's own key did not open it"

# The form again. Signed in to with a key: the folder button by the field, and
# in the dialog the public half, which stands for the key. Then the hop's own
# key by the hop's own button, and a second hop: past the hop's user and its
# x, the road's stop again.
action deep-x
key e
sleep 1
key Tab Tab Tab Tab Left
key Tab Tab Return
choose "$home/.ssh/id_ed25519.pub"
key Tab Tab Tab Return
choose "$home/.ssh/id_ed25519"
key Tab Tab Tab Return
typed shell-02
shot target/smoke/hops-form.png
key Return
sleep 2
block=$(grep -A4 '^Host deep-x$' "$config" | tr -s ' \n' ' ')
[ "$block" = "Host deep-x HostName 127.0.0.1 Port 2204 ProxyJump gate-x,shell-02 IdentityFile ~/.ssh/id_ed25519 " ] || fail "the key chosen and the road of two hops are in the config as '$block'"
[ "$(grep -c 'id_ed25519' "$config")" -eq 1 ] || fail "the hop's own key is in the ssh config, which has no word for it"
[ "$(rows "SELECT host, hop, key, user FROM jump_auth")" = "deep-x|gate-x|~/.ssh/id_ed25519|" ] || fail "the hop's own key is in the database as '$(rows "SELECT host, hop, key, user FROM jump_auth")'"
step "the file dialog's path is in the form with ~ for the home, the public half as its key; the hop's key is in the database only"

# Now the jump host opens to the key it was given here, the second hop to the
# default one, and behind them is a shell: three logins, and what is typed
# runs in shell-01's directory.
sleep 6
logins=$(accepted)
open_tab deep-x
run 'touch through-hops'
there 2204/through-hops "the tab through two jump hosts is not a shell on the host behind them"
shot target/smoke/hops-through.png
[ "$(accepted)" -eq $((logins + 3)) ] || fail "$(($(accepted) - logins)) logins for a road of two hops and its host, not 3"
key ctrl+shift+w
step "the key given to the hop opened it where its own did not: a shell through two jump hosts, three logins"

# The form as it stands, for the README: the key's folder button, the road as
# a picture, each hop and what it is logged in to with. In both languages.
action deep-x
key e
sleep 1
aside
shot docs/img/desktop-d6-host.png
key Escape
language
action deep-x
key e
sleep 1
aside
shot docs/img/desktop-d6-host.vi.png
key Escape
language

# A card's own switch is its first stop. Off: the database says so, the
# connection the dashboard kept is closed, and two rounds later nobody has
# logged in. On again: the next round logs in anew.
closed() { grep -c 'Connection closed by 127' "$home/sshd/log" || true; }
sleep 6
logins=$(accepted)
shut=$(closed)
action web-01
key Tab space
sleep 3
aside
shot target/smoke/watch-off.png
[ "$(rows "SELECT alias FROM hosts WHERE paused")" = web-01 ] || fail "the card's switch left the database with '$(rows "SELECT alias FROM hosts WHERE paused")' unwatched, not web-01"
[ "$(closed)" -eq $((shut + 1)) ] || fail "the connection kept for a host that is not watched any more was not closed ($(closed) closed, $shut before)"
sleep 11
[ "$(accepted)" -eq "$logins" ] || fail "a host that is not watched was logged in to: $(accepted) logins, $logins before"
key space
sleep 7
[ -z "$(rows "SELECT alias FROM hosts WHERE paused")" ] || fail "the card's switch, on again, left the host unwatched in the database"
[ "$(accepted)" -eq $((logins + 1)) ] || fail "the host that is watched again was not asked again: $(accepted) logins, $logins before"
step "a card's switch stopped the watching of its host: its connection closed, no login for two rounds; on again, one login"

# The same for every host at once, a row of the settings: every kept
# connection goes, and the dashboard says so at its top.
logins=$(accepted)
shut=$(closed)
setting 4
sleep 3
top
shot target/smoke/watch-paused.png
[ "$(rows "SELECT name FROM flags")" = watch_paused ] || fail "Pause watching is not in the database"
[ "$(closed)" -gt "$shut" ] || fail "Pause watching closed no connection"
sleep 11
[ "$(accepted)" -eq "$logins" ] || fail "with the watching paused somebody was logged in to: $(accepted) logins, $logins before"
setting 4
sleep 7
[ -z "$(rows "SELECT name FROM flags")" ] || fail "the watching, on again, is still paused in the database"
[ "$(accepted)" -gt "$logins" ] || fail "the watching, on again, asked nobody"
step "Pause watching stopped every round and closed what was kept; on again, the hosts are asked again"

# The two hosts go as they came; the hop's key goes with the host whose road
# it was on. shell-02 is the selected one again, as D9 left it.
action deep-x
key d Tab Return
sleep 2
[ -z "$(rows "SELECT * FROM jump_auth")" ] || fail "a deleted host left its hop's key in the database"
action gate-x
key d Tab Return
sleep 2
grep -qE '^Host (deep|gate)-x$' "$config" && fail "the hosts of the road are still in the config"
action shell-02
top
kill -0 "$app" 2>/dev/null || fail "the app died"
step "the app is still running after the file dialog, the road and the switches"

# --- D10: the cheatsheet, and the desktop's keys ------------------------------
# How many colours a strip of a screenshot has, one pixel high: x, y, width.
colours() { ffmpeg -loglevel error -i "$1" -vf "crop=$4:1:$2:$3" -f rawvideo -pix_fmt rgb24 - | od -An -v -tx1 -w3 | sort -u | wc -l; }
# No row of keys under the window: right of the selected host's address the
# status bar is its own ground and nothing else, at the height of its text,
# where "Enter Connect · Space select · v Quick peek · ..." used to be.
for picture in docs/img/desktop-d9-rail.png docs/img/desktop-d9-rail.vi.png; do
    [ "$(colours "$picture" 450 682 600)" -eq 1 ] || fail "$picture: there is something in the status bar where the row of keys was"
done
step "the status bar has no row of keys, in either language"

# The cheatsheet's panel is on screen left of its search glass, where the rail
# is while it is shut.
sheet() { [ "$(pixel "$1" 112 112)" = 1f2335 ]; }
key question
shot docs/img/desktop-d10-shortcuts.png
sheet docs/img/desktop-d10-shortcuts.png || fail "? did not open the cheatsheet"
# Its field has the keys: what is typed narrows the rows.
xdotool type --window "$win" tab
shot target/smoke/shortcuts-search.png
key Escape
shot target/smoke/shortcuts-shut.png
sheet target/smoke/shortcuts-shut.png && fail "Esc did not close the cheatsheet"
# The settings' last row; then Ctrl-/.
setting 6
shot target/smoke/shortcuts-button.png
sheet target/smoke/shortcuts-button.png || fail "the settings' row did not open the cheatsheet"
key Escape
key ctrl+slash
shot target/smoke/shortcuts-chord.png
sheet target/smoke/shortcuts-chord.png || fail "Ctrl-/ did not open the cheatsheet"
key Escape
# And F1, in Vietnamese.
language
key F1
shot docs/img/desktop-d10-shortcuts.vi.png
sheet docs/img/desktop-d10-shortcuts.vi.png || fail "F1 did not open the cheatsheet"
key Escape
language
step "the cheatsheet opened by ?, by the settings' row, by Ctrl-/ and by F1, and Esc closed it"

# Ctrl-T is a new tab: the palette asks where to. shell-01's port, as whoever
# runs this: a host has that address and is found first, and the address as it
# stands is the last row, one Up from the first.
key ctrl+t
typed 127.0.0.1:2204
shot target/smoke/new-tab.png
key Up Return
sleep 3
run 'touch new-tab'
there 2204/new-tab "Ctrl-T's palette did not open a tab on the address typed, shell-01's port"
# In a terminal the same keys are the shell's, as they were: Ctrl-T is
# readline's transpose (ab becomes ba), and ? is a character.
xdotool type --window "$win" --delay 20 'touch ab'
key ctrl+t
key Return
sleep 1
there 2204/ba "Ctrl-T in a terminal did not reach the shell: readline did not swap the two letters"
run "touch 'asked?'"
there '2204/asked?' "? in a terminal did not reach the shell"
shot target/smoke/terminal-keys.png
key ctrl+shift+w
step "Ctrl-T's palette connected to an address as it was typed; in a terminal Ctrl-T and ? stayed the shell's"

# From the last tab Ctrl-Tab comes round to the dashboard. There Ctrl-F is the
# search: what is typed is a filter, not the dashboard's keys. Enter there is
# the one card it leaves, and Enter on the card its tab.
key ctrl+Tab
key ctrl+f
xdotool type --window "$win" shell-01
shot target/smoke/find.png
key Return Return
sleep 3
run 'touch found'
there 2204/found "Ctrl-F did not put the keys in the hosts' search"
key ctrl+shift+w
step "Ctrl-Tab came round to the dashboard, and Ctrl-F there searched the hosts"

# --- D10: panes, focus mode, and the tabs under the pointer --------------------
# What the shell in front says of itself, in shell-01's directory: the columns
# of its terminal, and which terminal it is.
# The shell may be a moment behind the keys: its answer is waited for, and a
# shell that never gave one is in a picture.
cols() {
    run "stty size >$1"
    for _ in 1 2 3 4 5 6; do [ -s "$home/sshd/2204/$1" ] && break; sleep 1; done
    [ -s "$home/sshd/2204/$1" ] || fail "the shell in front did not answer stty: no $1 in shell-01's directory" >&2
    cut -d' ' -f2 "$home/sshd/2204/$1"
}
tty_of() { run "tty >$1"; cat "$home/sshd/2204/$1"; }
# The middle of the tab in front: in the strip of tabs it alone has the
# terminal's ground. Right of the rail, which has it too.
front_tab() {
    shot target/smoke/strip.png >/dev/null
    ffmpeg -loglevel error -i target/smoke/strip.png -vf "crop=820:1:260:11" -f rawvideo -pix_fmt rgb24 - | od -An -v -tx1 -w3 | tr -d ' ' |
        awk '$0 == "1a1b26" { if (!n++) first = NR; last = NR } END { print 260 + int((first + last) / 2) }'
}
# The panes are in the frame under the tabs: its left half, its right half.
LEFT=470 RIGHT=870
pane() { xdotool mousemove --window "$win" "$1" 420 click 1; sleep 0.5; }

open_tab shell-01
whole=$(cols whole)
first=$(tty_of first-tty)
# The tab in front, the third, let go on the left half of the first: it stands
# before it now, and the digits follow the strip. By Ctrl-A 2 and back by
# Ctrl-A 1, the shell that was the third tab is the first.
at=$(front_tab)
xdotool mousemove --window "$win" "$at" 24 mousedown 1
for x in $((at - 10)) $((at - 30)) 500 420 370 350; do xdotool mousemove --window "$win" "$x" 24; sleep 0.15; done
shot target/smoke/tab-moving.png
xdotool mouseup 1
sleep 1
key ctrl+a 2
key ctrl+a 1
[ "$(tty_of moved-tty)" = "$first" ] || fail "a tab let go on the first one in the strip is not the first now"
step "a tab let go on another in the strip stands before it, and the digits follow"
# Ctrl-Shift-D: another shell on shell-01 beside this one, and the keys are its.
key ctrl+shift+d
sleep 3
second=$(tty_of second-tty)
[ -n "$second" ] && [ "$second" != "$first" ] || fail "Ctrl-Shift-D: what was typed next went to the same shell ($first, $second)"
half=$(cols half)
[ "$half" -lt $((whole * 2 / 3)) ] || fail "the pane beside the first is $half columns of $whole: nothing was split"
aside
shot docs/img/desktop-d10-split.png
language
shot docs/img/desktop-d10-split.vi.png
language
# D12: the pane that has not the keys keeps its cursor, and it breathes: eight
# pictures of the left half of the frame, where nothing else moves, are not
# all one picture.
frames=$(for n in 1 2 3 4 5 6 7 8; do
    ffmpeg -loglevel error -f x11grab -draw_mouse 0 -video_size 400x560 -i "$DISPLAY+260,70" -frames:v 1 -f rawvideo -pix_fmt rgb24 - | cksum
    sleep 0.2
done | sort -u | wc -l)
[ "$frames" -gt 1 ] || fail "the cursor of the pane that has not the keys stands still: 8 pictures of it are one"
step "the pane beside the one in front shows its cursor, and it breathes ($frames different pictures of 8)"
# Broadcast across the two panes: the strip's button lit, each pane with its own switch.
cast
shot docs/img/desktop-d12-broadcast.png
# The settings took the keys: a click gives them back to the pane on the right,
# so both pictures are of the same state.
language
pane "$RIGHT"
shot docs/img/desktop-d12-broadcast.vi.png
language
pane "$RIGHT"
cast
# A click in a pane brings it to the front.
pane "$LEFT"
[ "$(tty_of clicked-tty)" = "$first" ] || fail "a click in the left pane did not give it the keys"
step "Ctrl-Shift-D split the tab: two shells side by side ($half of $whole columns each), a click chooses"

# The left pane's tab, dragged to the right edge of the panes: the two change places.
at=$(front_tab)
xdotool mousemove --window "$win" "$at" 24 mousedown 1
for x in $((at + 10)) $((at + 30)) 700 800 900 1000 1040; do xdotool mousemove --window "$win" "$x" 420; sleep 0.15; done
shot target/smoke/tab-dragged.png
xdotool mouseup 1
sleep 1
pane "$RIGHT"
[ "$(tty_of right-tty)" = "$first" ] || fail "the tab dragged to the right edge is not the pane on the right"
pane "$LEFT"
[ "$(tty_of left-tty)" = "$second" ] || fail "after the drag the other shell is not the pane on the left"
step "a tab dragged to an edge of the panes changed their places"

# The line between the two panes, held and taken to the right: the pane on the
# left is wider by it.
pane "$LEFT"
even=$(cols even)
xdotool mousemove --window "$win" 672 420 mousedown 1
for x in 690 720 760 800 820; do xdotool mousemove --window "$win" "$x" 420; sleep 0.1; done
xdotool mouseup 1
sleep 1
pane "$LEFT"
wider=$(cols wider)
[ "$wider" -gt "$even" ] || fail "the line between the panes did not follow the pointer: the left pane is $wider columns, $even before"
shot target/smoke/pane-resized.png
step "the line between two panes followed the pointer: the left one is $wider columns, from $even"

# Focus mode, by the key no shell is sent: the rail goes, and the panes are wider.
# The pane in front is the left one, which the line just made the wider.
# The pointer rests in a pane: where it rests on nothing with the rail there, a tab is now.
inside() { xdotool mousemove --window "$win" 550 520; sleep 0.5; }
language
key ctrl+shift+b
inside
shot docs/img/desktop-d10-focus.vi.png
key ctrl+shift+b
language
key ctrl+shift+b
inside
shot docs/img/desktop-d10-focus.png
wide=$(cols wide)
[ "$wide" -gt "$wider" ] || fail "focus mode: the pane is $wide columns, as before ($wider): the rail is still there"
# Plain Ctrl-B is the shell's, as it was: readline goes back a letter, and the X lands before the b.
xdotool type --window "$win" --delay 20 'touch ab'
key ctrl+b
xdotool type --window "$win" X
key Return
sleep 1
there 2204/aXb "Ctrl-B in a terminal did not reach the shell"
[ "$(cols still-wide)" = "$wide" ] || fail "Ctrl-B in a terminal left focus mode"
# Esc is the shell's there too: readline takes it for Meta and the key after
# it with it, here a Ctrl-U that is nobody's. Ctrl-A z is the way back from
# the keyboard.
key Escape ctrl+u
[ "$(cols esc-wide)" = "$wide" ] || fail "Esc in a terminal left focus mode"
key ctrl+a z
[ "$(cols back)" = "$wider" ] || fail "Ctrl-A z did not bring the rail back"
step "focus mode by Ctrl-Shift-B: no rail ($wide columns in the pane in front, from $wider); Ctrl-B and Esc stayed the shell's, Ctrl-A z came back"

# The wheel's button on a tab closes it: the pane that is left has the whole frame again.
xdotool mousemove --window "$win" "$(front_tab)" 24 click 2
sleep 1
aside
[ "$(cols alone)" = "$whole" ] || fail "a middle click on the tab in front did not close its pane"
[ "$(tty_of alone-tty)" = "$first" ] || fail "the pane left after the close is not the other shell"
step "a middle click closed the tab of one pane; the other took the frame"

# --- D11: the shell's own keys, the right button, the rail narrow --------------
# shell-01's tab is in front. Ctrl-W there is readline's: the word before the
# cursor goes, and the tab does not. Ctrl-K cuts from the cursor to the end.
xdotool type --window "$win" --delay 20 'touch word-kept word-gone'
key ctrl+w
key Return
sleep 1
there 2204/word-kept "after Ctrl-W in a terminal the shell is not there to run what was typed: the tab was closed"
[ ! -e "$home/sshd/2204/word-gone" ] || fail "Ctrl-W in a terminal did not reach the shell: the word before the cursor was not deleted"
xdotool type --window "$win" --delay 20 'touch cutXYZ'
key Left Left Left ctrl+k Return
sleep 1
there 2204/cut "Ctrl-K in a terminal did not reach the shell: the end of the line was not cut"
step "in a terminal Ctrl-W deleted a word and Ctrl-K cut a line: the shell's keys, and the tab is still there"

# A tab whose shell runs something is asked about before it goes. Cancel has
# the focus: Enter keeps the tab, and its shell is there for the next line.
run 'sleep 300'
sleep 6
key ctrl+shift+w
shot target/smoke/close-busy.png
key Return
key ctrl+c
run 'touch asked-first'
there 2204/asked-first "a tab that was running a command was closed without a yes"
step "Ctrl-Shift-W on a tab that runs a command asked first, and Enter kept the tab"

# The right button in a terminal: its menu's first row that can be chosen is
# Paste (nothing is selected to copy), and what the clipboard has is typed.
if command -v xclip >/dev/null; then
    printf 'touch pasted-by-menu' | xclip -selection clipboard
    xdotool mousemove --window "$win" 700 400 click 3
    sleep 0.5
    shot target/smoke/terminal-menu.png
    key Return
    sleep 1
    key Return
    sleep 1
    there 2204/pasted-by-menu "Paste in the terminal's menu did not type what the clipboard had"
    step "the right button in a terminal pasted the clipboard into the shell"
    # D12: the same by Ctrl-Shift-V, once: the web view's own paste did not come after it.
    printf 'touch pasted-by-key' | xclip -selection clipboard
    key ctrl+shift+v
    sleep 1
    shot target/smoke/pasted-by-key.png
    key Return
    sleep 1
    there 2204/pasted-by-key "Ctrl-Shift-V did not type what the clipboard had into the terminal in front"
    [ ! -e "$home/sshd/2204/pasted-by-keytouch" ] || fail "Ctrl-Shift-V pasted twice"
    step "Ctrl-Shift-V pasted the clipboard into the terminal in front, once"
fi

# The rail narrow, by its corner's button: the terminal is wider by what the
# rail gave up, and the web view keeps the choice.
full=$(cols rail-full)
xdotool mousemove --window "$win" 218 34 click 1
sleep 1
terminal
narrow=$(cols rail-narrow)
[ "$narrow" -gt "$full" ] || fail "the rail's corner did not make it narrow: the terminal is $narrow columns, $full before"
[ "$(kept vssh.rail)" = compact ] || fail "the web view kept the rail as $(kept vssh.rail), not compact"
RAIL=28
key ctrl+a 0
# The hosts' search still has what Ctrl-F was given: Esc in it empties it.
key ctrl+f Escape
top
shot docs/img/desktop-d11-compact.png
language
top
shot docs/img/desktop-d11-compact.vi.png
language
# And all of it again, by the same button, which is under the mark now.
xdotool mousemove --window "$win" 28 76 click 1
sleep 1
RAIL=120
[ "$(kept vssh.rail)" = full ] || fail "the web view kept the rail as $(kept vssh.rail), not full"
aside
step "the rail narrow: its icons, a terminal of $narrow columns from $full, and the web view kept it"

# The right button on a card: the palette brings web-01's into view, low in
# the right column, and Copy address is the fifth row of its menu. Then the
# first tab, shell-01's, in front again: it is what the app is closed with.
if command -v xclip >/dev/null; then
    key ctrl+k
    xdotool type --window "$win" web-01
    key Return
    sleep 1
    xdotool mousemove --window "$win" 800 630 click 3
    sleep 0.5
    shot target/smoke/card-menu.png
    key Down Down Down Down Return
    sleep 1
    copied=$(xclip -o -selection clipboard)
    [ "$copied" = "127.0.0.1:2201" ] || fail "Copy address in a card's menu copied '$copied'"
    step "the menu of a card copied its host's address"
fi
key ctrl+a 1

# The app again: the vault is there and shut, and it is asked for first thing.
again() {
    kill "$app"
    wait "$app" 2>/dev/null || true
    env -u SSH_AUTH_SOCK HOME="$home" WEBKIT_DISABLE_DMABUF_RENDERER=1 target/release/vssh-desktop &
    app=$!
    win=$(timeout 60 xdotool search --sync --onlyvisible --name '^vssh$' | head -n 1)
    [ -n "$win" ] || fail "no window named vssh after the app was started again"
    xdotool windowfocus "$win"
    sleep 5
}
again
shot target/smoke/vault-locked.png
for guess in one two three; do
    typed "$guess"
    key Return
    sleep 1
done
shot target/smoke/vault-wrong.png
typed 'smoke master'
key Return
sleep 6
shot target/smoke/vault-unlocked.png
# What was open is open again, once the vault was answered: shell-01's tab was
# the one in front, and is a shell with the keys again.
run 'touch open-again'
there 2204/open-again "the tab that was open when the app was closed is not open again"
step "the app came back with the tabs it had, after the vault's question"

# The same refusal in Vietnamese. The question is modal and comes first, so
# the language is chosen before the app goes: the web view keeps it.
language
sleep 2
again
typed 'one'
key Return
sleep 1
shot target/smoke/vault-wrong.vi.png
typed 'smoke master'
key Return
sleep 3

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the vault, the host form and the key setup"

# --- D7: the bar's two rows, the Day theme, streamer mode ---------------------
# The app is in Vietnamese.
# The two switches, rows of the settings.
streamer() { setting 3; }
theme() { setting 2; }
pixels() { file=$1; shift; while [ $# -ge 2 ]; do printf '%s ' "$(pixel "$file" "$1" "$2")"; shift 2; done; }

# With cards marked the second row of the bar has two buttons more, in
# Vietnamese the longest it gets: it is still two rows, and the tags give way.
# Both start where the search does: left of x 276 the band is the band's own
# in both rows, and at x 276 both have their line. Under the second row the
# band is its own down to the line it ends with: no third row, and no card
# shows through, though the cards are scrolled under it. (The tabs that came
# back with the app are the window's top row: the bar is under them.)
for name in shell-01 shell-02; do mark "$name"; done
aside
shot target/smoke/toolbar-wrap.vi.png
rows=$(pixels target/smoke/toolbar-wrap.vi.png 275 85 275 135 500 164 500 171)
[ "$rows" = "1a1b26 1a1b26 1a1b26 2a2f45 " ] || fail "the bar over the cards is not two rows from one margin, or it took a third: $rows"
case "$(pixels target/smoke/toolbar-wrap.vi.png 276 85 276 135)" in
*1a1b26*) fail "the bar's two rows do not both start at x 276" ;;
esac
step "with cards marked, the Vietnamese bar is two rows from one margin, and no third"
for name in shell-01 shell-02; do mark "$name"; done

# Day: the window's background, and the panel of the first card (its padding,
# left of what is in it).
theme
first_card
top
shot docs/img/desktop-d7-day.vi.png
day=$(pixels docs/img/desktop-d7-day.vi.png 264 300 285 500)
[ "$day" = "e1e2e7 d0d5e3 " ] || fail "in Day the background and the card are $day, not e1e2e7 d0d5e3"
[ "$(kept vssh.theme)" = day ] || fail "the theme the web view kept is $(kept vssh.theme), not day"
step "the Day theme: background #e1e2e7, cards #d0d5e3, and the web view kept the choice"
# A terminal stays dark in Day, inside the theme's border.
open_tab shell-01
run 'echo dark in the day'
shot target/smoke/day-terminal.vi.png
term=$(pixels target/smoke/day-terminal.vi.png 244 400 900 500)
[ "$term" = "a1a6c5 1a1b26 " ] || fail "in Day the terminal's border and ground are $term, not a1a6c5 1a1b26"
step "in Day the terminal is still Night's, with the theme's border around it"
# The tab that goes leaves the front to the one beside it, of those that came
# back with the app: the dashboard is asked for by its own key.
key ctrl+shift+w
key ctrl+a 0
language
first_card
# Twice: the dashboard came back far down, at the card of the host whose tab went.
top
top
shot docs/img/desktop-d7-day.png

# Streamer mode, in Night again. What is shown is all that changes: the
# config is the file it was, and a tab reaches the host at its real address.
theme
before=$(cksum <"$config")
streamer
first_card
top
shot docs/img/desktop-d7-streamer.png
language
first_card
top
shot docs/img/desktop-d7-streamer.vi.png
[ "$(kept vssh.streamer_mode)" = true ] || fail "the web view kept streamer_mode as $(kept vssh.streamer_mode), not true"
[ "$(kept vssh.theme)" = night ] || fail "the theme the web view kept is $(kept vssh.theme), not night"
open_tab shell-02
run 'touch streamed'
there 2205/streamed "in streamer mode a tab did not reach shell-02"
shot target/smoke/streamer-terminal.vi.png
key ctrl+shift+w
[ "$(cksum <"$config")" = "$before" ] || fail "streamer mode changed the ssh config"
grep -qE '203\.0\.113\.|198\.51\.100\.' "$config" "$home/.ssh/known_hosts" && fail "a made-up address was written down"
step "streamer mode: kept by the web view, the config untouched, a tab still on the real host"

# The app again: both switches are as they were left.
again
typed 'smoke master'
key Return
sleep 3
aside
shot target/smoke/streamer-restart.vi.png
[ "$(kept vssh.streamer_mode)" = true ] || fail "streamer mode did not survive the app being started again"
step "streamer mode is still on when the app comes back"

kill -0 "$app" 2>/dev/null || { echo "FAIL: the app died"; exit 1; }
step "the app is still running after the Day theme and streamer mode"

# --- D15: the update, from the line that says a newer version is out ----------
# An older build of this source (the first block made it, as 0.0.1), over a
# HOME of its own: to it the release GitHub has is news. What is real: the
# question to GitHub, the download with wget, dpkg-deb's reading of the file,
# the app ending and starting again. What is not: pkexec, dpkg-query and
# xdg-open are stand-ins on the PATH of this one app, so nothing is installed
# on this machine and no folder opens on a display nobody looks at; and wget
# is held to 1.5 MiB a second, so that the download can be looked at.
kill "$app"
wait "$app" 2>/dev/null || true
latest=$(wget -q -T 20 -O - https://api.github.com/repos/DucLong06/vssh/releases/latest | python3 -c '
import json, sys
release = json.load(sys.stdin)
ver = release["tag_name"].lstrip("v")
print(ver, next(a["size"] for a in release["assets"] if a["name"] == f"vssh-desktop_{ver}_amd64.deb"))') || latest=
if [ -z "$latest" ]; then
    echo "SKIP: GitHub does not answer from here, or its latest release has no .deb: the update was not tried"
    echo "gui smoke: ok, but for the update (now look at the screenshots)"
    exit 0
fi
ver=${latest% *}
bytes=${latest#* }
up="$home/update"
deb="$up/home/.local/share/vssh/update/vssh-desktop_${ver}_amd64.deb"
mkdir -p "$up/bin" "$up/home"
[ -d "$HOME/.local/share/fonts" ] && mkdir -p "$up/home/.local/share" && ln -s "$HOME/.local/share/fonts" "$up/home/.local/share/fonts"
# Whose the app's binary is: the package's, says this one.
cat >"$up/bin/dpkg-query" <<EOF
#!/bin/sh
echo "\$*" >>"$up/dpkg-query.log"
echo "vssh-desktop: \$2"
EOF
cat >"$up/bin/wget" <<EOF
#!/bin/sh
echo "\$*" >>"$up/wget.log"
[ -e "$up/offline" ] && { echo "wget: unable to resolve host address 'github.com'" >&2; exit 4; }
exec $(command -v wget) --limit-rate=1500k "\$@"
EOF
# What it was asked to run, and then: dpkg's refusal, nobody to ask for the
# password, or done.
cat >"$up/bin/pkexec" <<EOF
#!/bin/sh
echo "\$*" >>"$up/pkexec.log"
case \$(cat "$up/pkexec") in
dpkg) echo "dpkg: error: dpkg frontend lock was locked by another process with pid 4242" >&2; exit 2 ;;
nobody) exit 127 ;;
esac
EOF
cat >"$up/bin/xdg-open" <<EOF
#!/bin/sh
echo "\$*" >>"$up/opened"
EOF
chmod +x "$up"/bin/*
lines() { [ ! -e "$1" ] && echo 0 || wc -l <"$1"; }
# Waits until a stand-in was run that many times.
ran() {
    tries=0
    until [ "$(lines "$up/$1.log")" -ge "$2" ]; do
        tries=$((tries + 1))
        [ "$tries" -le 120 ] || fail "$3"
        sleep 0.5
    done
}
# The button right of the line: Update, or Retry after a failure.
update() { xdotool mousemove --window "$win" 1040 682 click 1; }

# The web view is given no desktop to ask for a proxy: it reads the
# environment's, as wget does.
env -u SSH_AUTH_SOCK -u XDG_CURRENT_DESKTOP HOME="$up/home" PATH="$up/bin:$PATH" WEBKIT_DISABLE_DMABUF_RENDERER=1 target/smoke-old/release/vssh-desktop &
app=$!
win=$(timeout 60 xdotool search --sync --onlyvisible --name '^vssh$' | head -n 1)
[ -n "$win" ] || fail "no window of the older build"
xdotool windowfocus "$win"
ran dpkg-query 1 "the older build never heard of v$ver: the web view got no answer from GitHub"
aside
shot target/smoke/update-offered.png
[ ! -e "$up/wget.log" ] && [ ! -e "${deb%/*}" ] || fail "something was fetched before Update was clicked"
step "an older build says v$ver is out, has a button for it, and has fetched nothing"

# No network: a short error and Retry. Nothing is installed.
touch "$up/offline"
update
ran wget 1 "Update did not start a download"
aside
shot target/smoke/update-download-failed.png
[ "$(cat "$up/wget.log")" = "-nv --tries=2 --timeout=30 -O $deb https://github.com/DucLong06/vssh/releases/download/v$ver/vssh-desktop_${ver}_amd64.deb" ] || fail "the download was asked for as: $(cat "$up/wget.log")"
[ ! -e "$up/pkexec.log" ] || fail "a download that failed was installed"
step "a download that fails is an error with Retry, and nothing is installed"

# Retry, with the network back: the real .deb from GitHub, whole, and read by
# dpkg-deb. dpkg then refuses: an error again, and the file is kept.
rm "$up/offline"
echo dpkg >"$up/pkexec"
update
sleep 1.5
shot target/smoke/update-downloading.png
ran pkexec 1 "the .deb was fetched but its install was never asked for"
aside
shot target/smoke/update-install-failed.png
[ "$(size "$deb")" = "$bytes" ] || fail "the .deb is $(size "$deb") bytes, the release says $bytes"
[ "$(dpkg-deb -f "$deb" Package Version | tr '\n' ' ')" = "Package: vssh-desktop Version: $ver " ] || fail "what was fetched is not vssh-desktop $ver"
[ "$(cat "$up/pkexec.log")" = "env DEBIAN_FRONTEND=noninteractive dpkg -i $deb" ] || fail "the install was asked for as: $(cat "$up/pkexec.log")"
step "Retry fetched vssh-desktop_${ver}_amd64.deb from GitHub, $bytes bytes, and asked pkexec for dpkg -i; dpkg's refusal is an error with Retry"

# Nobody to ask for the password: the folder is opened, and the dialog has the
# line that installs it by hand.
echo nobody >"$up/pkexec"
update
ran pkexec 2 "Retry after dpkg's refusal did not ask again"
sleep 1
shot target/smoke/update-by-hand.png
[ "$(cat "$up/opened")" = "${deb%/*}" ] || fail "the folder opened is '$(cat "$up/opened" 2>/dev/null)', not the .deb's"
[ "$(size "$deb")" = "$bytes" ] || fail "the .deb to install by hand is not there"
key Escape
step "with nobody to ask for the password the .deb's folder is opened and nothing is installed"

# Installed: the dialog offers to restart, and the app that comes back is
# another process, which asks GitHub again.
echo ok >"$up/pkexec"
update
ran pkexec 3 "Update after the dialog did not ask again"
sleep 1
shot target/smoke/update-installed.png
[ ! -e "$deb" ] || fail "the .deb is still there after it was installed"
# Later has the focus; Restart is the next.
key Tab Return
tries=0
while kill -0 "$app" 2>/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -le 40 ] || fail "Restart did not end the app"
    sleep 0.5
done
was=$app
win=$(timeout 60 xdotool search --sync --onlyvisible --name '^vssh$' | head -n 1)
[ -n "$win" ] || fail "no window after Restart"
app=$(xdotool getwindowpid "$win")
[ "$app" != "$was" ] || fail "the window after Restart is the old process's"
ran dpkg-query 2 "the app that came back did not ask GitHub again"
aside
shot target/smoke/update-restarted.png
step "Restart ended the app and started it again: pid $was went, pid $app has the window"
echo "gui smoke: ok (now look at the screenshots)"
