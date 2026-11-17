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
# Screenshots land in docs/img/desktop-d2*.png, desktop-d3*.png,
# desktop-d4*.png, desktop-d5*.png and desktop-d6*.png (.vi.png: the
# Vietnamese UI; d3-fail: a connection that fails, d3-closed: one that drops;
# d4-sftp: the file view with transfers under way; d5-batch: what four hosts
# answered to one command; d6-vault: the vault's manager, d6-host: the host
# form); those of the steps in between, in target/smoke/.
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

# TokyoNight Night is what is on screen: the window's background in the
# gutter left of the cards, and the panel inside the first card.
night="$(pixel docs/img/desktop-d2.png 264 300) $(pixel docs/img/desktop-d2.png 640 140)"
[ "$night" = "1a1b26 1f2335" ] || { echo "FAIL: background and card are $night, not 1a1b26 1f2335"; exit 1; }
step "the Night theme: background #1a1b26, cards #1f2335"

# One connection per host that lets us in, however many rounds were asked.
logins=$(accepted)
[ "$logins" -eq 3 ] || { echo "FAIL: $logins logins after three rounds, expected 3 (one per host)"; cat "$home/sshd/log"; exit 1; }
step "three rounds of metrics over three kept connections ($logins logins)"
grep -q '127.0.0.2' "$home/.ssh/known_hosts" && { echo "FAIL: the dashboard recorded new-box's key on its own"; exit 1; }
step "the host nobody connected to was left alone"

xdotool windowfocus "$win"
# / is the search box; four Tabs on (Group by tag, Add host, the sort) is the first card.
first_card() { xdotool key --window "$win" slash Tab Tab Tab Tab; }
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

# And by resting the pointer on a card: db-replica, which is down.
xdotool mousemove --window "$win" 880 140
sleep 1
shot target/smoke/hover.png
xdotool mousemove --window "$win" 600 20

# A person asking about the host nobody connected to: the last card, five
# tab stops per card further on (the card, Edit, Forwards, Files, Connect). That is
# a decision: its key goes on record, as with `v` in the TUI, and from then
# on it is polled like the others.
first_card
xdotool key --window "$win" --repeat 35 Tab
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
# The palette finds the host's card; Enter on the card opens its tab.
open_tab() {
    key ctrl+k
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
# Both tabs broadcast: the one in front has the full yellow border along its
# top, the one behind a dimmer one.
front=$(pixel docs/img/desktop-d3-terminal.png 450 6)
behind=$(pixel docs/img/desktop-d3-terminal.png 700 6)
[ "$front" = e0af68 ] && [ "$behind" != "$front" ] && [ "$behind" != 15161e ] ||
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
# Once was once. Trust puts the key the host has now on record.
key ctrl+w
open_tab moved-box
key Tab
key Tab
key Return
sleep 3
[ "$(grep -F '[127.0.0.3]:2201' "$home/.ssh/known_hosts" | cut -d' ' -f2,3)" = "$key" ] || { echo "FAIL: Trust did not replace the key on record"; cat "$home/.ssh/known_hosts"; exit 1; }
step "Trust: the key on record is the one the host has now"
key ctrl+w

# A jump host that is down, second of two: the first is logged in to, and
# the dialog says where it stopped.
logins=$(accepted)
open_tab far-box
shot docs/img/desktop-d3-fail.png
[ "$(accepted)" -eq $((logins + 1)) ] || { echo "FAIL: far-box's first jump host was not logged in to"; exit 1; }
step "a jump chain that breaks at hop 2 got through hop 1"
key Escape
# The same in Vietnamese. The dialog is modal: the language goes first.
key ctrl+w
language
open_tab far-box
shot docs/img/desktop-d3-fail.vi.png
key Escape
key ctrl+w
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
red=$(pixel docs/img/desktop-d3-closed.png 960 43)
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
    key ctrl+k
    xdotool type --window "$win" "$1"
    key Return
    key f
    sleep 4
}
# The rows of the two panes: 23 px each, the middle of the first at y 120.
row() { echo $((120 + 23 * $1)); }
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

# The right button under the host's rows: New folder is all there is to ask.
xdotool mousemove --window "$win" "$REMOTE" 540 click 3
sleep 0.5
shot target/smoke/files-menu.png
key Return
xdotool type --window "$win" made-here
key Return
sleep 3
[ -d "$files/made-here" ] || fail "the menu's New folder made nothing on the host"
step "the right button's menu made a folder on the host"

# The host's rows now: made-here, releases, shared, deploy.log, nginx.conf,
# notes.txt. On a file the menu is Copy, New folder, Rename, Delete.
click_row "$REMOTE" 3 3
key Down Down Return
xdotool type --window "$win" deploy.old
key Return
sleep 3
[ -e "$files/deploy.old" ] && [ ! -e "$files/deploy.log" ] || fail "Rename did not rename deploy.log"
# Delete asks first: Cancel has the focus, Delete is next.
click_row "$REMOTE" 4 3
key Up Return
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
xdotool mousemove --window "$win" 1041 609 click 1
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
    key ctrl+k
    xdotool type --window "$win" "$1"
    key Return
    key space
}
for name in shell-01 shell-02 cache-01 legacy; do mark "$name"; done
# The pointer where it rests on nothing.
xdotool mousemove --window "$win" 600 20

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
    key ctrl+k
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
    key ctrl+k
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
shot docs/img/desktop-d6-host.png
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
# Preview, bastion-x, inner-x (unticked), Cancel, Import.
key Tab Tab Tab space
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
shot docs/img/desktop-d6-host.vi.png
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
sleep 3
shot target/smoke/vault-unlocked.png

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
echo "gui smoke: ok (now look at the screenshots)"
