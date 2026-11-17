#!/bin/sh
# The desktop app as a .deb, built by hand: for a machine where `tauri build`
# (what .github/workflows/desktop-release.yml runs) has no inotify watches
# left, or no network to fetch its bundling tools with.
#
#   scripts/desktop-deb.sh 0.2.0   ->  target/deb/vssh-desktop_0.2.0_<arch>.deb
#
# Check it with scripts/deb-smoke.sh.
set -eu
cd "$(dirname "$0")/.."
ver=${1:?usage: desktop-deb.sh VERSION}
# It goes into JSON and into a control file; tauri wants semver anyway.
case $ver in
    *[!0-9A-Za-z.+-]* | [!0-9]*) echo "FAIL: '$ver' is not a version like 0.2.0"; exit 1 ;;
esac
for tool in npm cargo dpkg-deb; do
    command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed"; exit 1; }
done

# To tauri a plain `cargo build` is a dev build: with a devUrl in the config
# the window would ask localhost:1420 for its page. TAURI_CONFIG is merged
# over tauri.conf.json (RFC 7396: null takes the key out), so the file is
# left alone, and the version of the bundle comes from here as in a release.
(cd crates/vssh-desktop && npm run build)
TAURI_CONFIG="{\"version\":\"$ver\",\"build\":{\"devUrl\":null}}" cargo build --release -p vssh-desktop

arch=$(dpkg --print-architecture)
pkg=target/deb/vssh-desktop_${ver}_${arch}
rm -rf "$pkg" "$pkg.deb"
install -Dm755 target/release/vssh-desktop "$pkg/usr/bin/vssh-desktop"
install -Dm644 crates/vssh-desktop/src-tauri/icons/128x128.png \
    "$pkg/usr/share/icons/hicolor/128x128/apps/vssh-desktop.png"
mkdir -p "$pkg/usr/share/applications" "$pkg/DEBIAN"
cat > "$pkg/usr/share/applications/vssh-desktop.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=vssh
Comment=SSH connection manager
Exec=vssh-desktop
Icon=vssh-desktop
Terminal=false
Categories=Network;Utility;
DESKTOP
cat > "$pkg/DEBIAN/control" <<CTRL
Package: vssh-desktop
Version: $ver
Section: utils
Priority: optional
Architecture: $arch
Maintainer: Hoang Duc Long <hoangduclongg@gmail.com>
Depends: libwebkit2gtk-4.1-0, libgtk-3-0
Description: Desktop SSH connection manager
 Host dashboard with live stats, terminal tabs with broadcast typing, SFTP
 dual-pane, snippets, batch runs, port forwards and an encrypted vault.
 The desktop frontend of vssh, on the same core as the terminal app.
CTRL
dpkg-deb --build --root-owner-group "$pkg"
echo "ok: $pkg.deb"
