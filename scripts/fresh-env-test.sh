#!/bin/bash
# Fresh-environment install test: bare ubuntu:22.04 (no fonts, no icon themes)
# installs both vssh TUI (.deb) and vssh-desktop (.deb), runs each, captures proof.
# Usage: bash scripts/fresh-env-test.sh  (needs docker; artifacts -> /tmp/vssh-fresh)
set -u
export https_proxy=http://proxy.fpt.vn:80 http_proxy=http://proxy.fpt.vn:80
REPO=/mnt/data/Project/vssh
OUT=/tmp/vssh-fresh
mkdir -p "$OUT"

# 1. fetch the TUI release .deb (v0.1.2, musl static)
TUI_DEB=$(ls "$OUT"/vssh_*_amd64.deb 2>/dev/null | head -1)
if [ -z "$TUI_DEB" ]; then
  curl -sL -o "$OUT/vssh_tui.deb" https://github.com/DucLong06/vssh/releases/download/v0.1.2/vssh_0.1.2_amd64.deb && TUI_DEB="$OUT/vssh_tui.deb"
fi
ls -la "$TUI_DEB"

# 2. run the bare container with both debs mounted
docker run --rm \
  -v "$TUI_DEB":/pkgs/vssh-tui.deb:ro \
  -v "$REPO/target/deb/vssh-desktop_0.2.0_amd64.deb":/pkgs/vssh-desktop.deb:ro \
  -v "$OUT":/out \
  -e http_proxy=http://proxy.fpt.vn:80 -e https_proxy=http://proxy.fpt.vn:80 \
  ubuntu:22.04 bash -s <<'REMOTE'
set -x
export DEBIAN_FRONTEND=noninteractive
echo "== what a bare image has =="
fc-list 2>/dev/null | wc -l; ls /usr/share/icons 2>/dev/null; ls /usr/share/fonts 2>/dev/null
apt-get update -qq
apt-get install -y -qq /pkgs/vssh-tui.deb /pkgs/vssh-desktop.deb xvfb x11-utils xdotool ffmpeg tmux fonts-noto-core > /tmp/apt.log 2>&1 || { tail -30 /tmp/apt.log; exit 1; }
vssh --version; vssh-desktop --version
echo "== fonts present after install? (fonts-noto only) =="
fc-list | wc -l

# TUI test in tmux (text render — tofu would show as ? in the capture)
tmux new-session -d -s t -x 100 -y 30 'HOME=/tmp/h vssh --lang vi; sleep 600'
sleep 3; tmux capture-pane -t t -p > /out/tui.txt; tmux kill-session -t t
head -12 /out/tui.txt

# Desktop test under xvfb BEFORE any UI font exists beyond noto-core
export HOME=/tmp/h; mkdir -p $HOME
xvfb-run -a -s "-screen 0 1100x700x24" sh -c 'vssh-desktop >/tmp/app.log 2>&1 & AP=$!; sleep 12; ffmpeg -loglevel error -y -f x11grab -draw_mouse 0 -video_size 1100x700 -i $DISPLAY -frames:v 1 /out/desktop-fresh.png; kill $AP'
ls -la /out/desktop-fresh.png /out/tui.txt
REMOTE
echo "EXIT:$?"
