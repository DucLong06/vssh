#!/bin/sh
# Smoke test of a desktop .deb: is it the version it says, and does its window
# show the app? A process that is alive proves neither.
#
#   scripts/deb-smoke.sh DEB VERSION [SCREENSHOT]
#
# In a release (.github/workflows/desktop-release.yml) it runs as root in a
# bare ubuntu:22.04 container with VSSH_DEB_INSTALL=1: the package is
# installed with apt, so what it forgot to depend on is missing. Without that
# variable the package is only unpacked, and runs on this machine's libraries.
#
# The render gate, on a virtual display and an empty HOME:
#   - a window named vssh opens;
#   - the database appears: the page loaded, its script ran, and the first
#     command it sent the core was answered (a window that shows nothing, or
#     an error page of the web view, never gets there);
#   - the screenshot has more colours than a window that is only painted.
set -eu
deb=${1:?usage: deb-smoke.sh DEB VERSION [SCREENSHOT]}
ver=${2:?usage: deb-smoke.sh DEB VERSION [SCREENSHOT]}
shot=${3:-target/smoke/deb-smoke.png}
SIZE=1100x700
# A window with nothing in it is one colour, the web view's own error page
# some 80; the empty dashboard is a thousand.
MIN_COLOURS=256
fail() { echo "FAIL: $1"; exit 1; }
step() { echo "ok: $1"; }

if [ -z "${VSSH_SMOKE_DISPLAY:-}" ]; then
    if [ -n "${VSSH_DEB_INSTALL:-}" ]; then
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -q
        # A font: the image has none, and text is most of what there is to see.
        apt-get install -y -q --no-install-recommends "$(realpath "$deb")" \
            xvfb xauth xdotool ffmpeg fonts-dejavu-core
        bin=/usr/bin/vssh-desktop
        step "apt installed the package and what it depends on"
    else
        for tool in dpkg-deb xvfb-run xdotool ffmpeg; do
            command -v "$tool" >/dev/null || fail "$tool is not installed"
        done
        root=$(mktemp -d)
        trap 'rm -rf "$root"' EXIT
        dpkg-deb -x "$deb" "$root"
        bin=$root/usr/bin/vssh-desktop
    fi
    got=$(dpkg-deb -f "$deb" Version)
    [ "$got" = "$ver" ] || fail "the package says version '$got', not '$ver'"
    step "the package is version $ver"
    got=$("$bin" --version)
    [ "$got" = "vssh-desktop $ver" ] || fail "--version says '$got', not 'vssh-desktop $ver'"
    step "--version says $ver, with no display"
    mkdir -p "$(dirname "$shot")"
    # Not exec: the trap above still has the unpacked package to remove.
    VSSH_SMOKE_DISPLAY=1 VSSH_SMOKE_BIN=$bin xvfb-run -a -s "-screen 0 ${SIZE}x24" "$0" "$@"
    exit
fi

home=$(mktemp -d)
app=
trap 'kill $app 2>/dev/null || true; wait 2>/dev/null || true; rm -rf "$home" 2>/dev/null || { sleep 2; rm -rf "$home"; }' EXIT
# No GPU on a virtual display. No agent: nothing of the user's is offered.
env -u SSH_AUTH_SOCK HOME="$home" WEBKIT_DISABLE_DMABUF_RENDERER=1 "$VSSH_SMOKE_BIN" &
app=$!

win=$(timeout 60 xdotool search --sync --onlyvisible --name '^vssh$' | head -n 1) || true
[ -n "$win" ] || fail "no window named vssh after 60 s"
step "the window opened"

tries=0
until [ -f "$home/.local/share/vssh/vssh.db" ]; do
    tries=$((tries + 1))
    if [ "$tries" -gt 60 ]; then
        ffmpeg -loglevel error -y -f x11grab -draw_mouse 0 -video_size "$SIZE" -i "$DISPLAY" -frames:v 1 "$shot" || true
        fail "the page never asked the core for the hosts: no UI (see $shot)"
    fi
    sleep 0.5
done
step "the page loaded and the core answered it"

# The answer is drawn a moment after it arrives.
sleep 2
ffmpeg -loglevel error -y -f x11grab -draw_mouse 0 -video_size "$SIZE" -i "$DISPLAY" -frames:v 1 "$shot"
colours=$(ffmpeg -loglevel error -i "$shot" -f rawvideo -pix_fmt rgb24 - | od -An -v -tx1 -w3 | sort -u | wc -l)
[ "$colours" -ge "$MIN_COLOURS" ] || fail "the window is blank: $colours colours in $shot"
kill -0 "$app" 2>/dev/null || fail "the app did not stay up"
step "the window shows the app: $colours colours in $shot"
echo "deb smoke: ok"
