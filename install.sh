#!/bin/sh
# vssh installer. Messages are in English or Vietnamese.
#
#   curl -fsSL https://raw.githubusercontent.com/DucLong06/vssh/main/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --lang vi
#   sh install.sh [--lang en|vi] [--prefix DIR]
#
# Run from a vssh checkout it builds that checkout. Otherwise it downloads the
# latest release binary and, if there is none for this machine, builds from
# source with cargo. Running it again updates an existing install.
set -eu

REPO="DucLong06/vssh"
PREFIX="${VSSH_PREFIX:-$HOME/.local}"
LANG_ARG=""
SHOW_HELP=0

# The first of --lang, VSSH_LANG, LC_ALL, LC_MESSAGES, LANG that is set
# decides: Vietnamese if it starts with "vi", English otherwise.
detect_lang() {
    for value in "$LANG_ARG" "${VSSH_LANG:-}" "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}"; do
        case "$value" in
            "") ;;
            vi* | VI*) echo vi; return ;;
            *) echo en; return ;;
        esac
    done
    echo en
}

# say ENGLISH VIETNAMESE [printf arguments...]
say() {
    if [ "$L" = vi ]; then fmt=$2; else fmt=$1; fi
    shift 2
    # shellcheck disable=SC2059  # the format strings are the literals below
    printf "$fmt\n" "$@"
}

die() {
    say "$@" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --lang) LANG_ARG="${2:-}"; shift ;;
        --lang=*) LANG_ARG="${1#--lang=}" ;;
        --prefix) PREFIX="${2:-}"; shift ;;
        --prefix=*) PREFIX="${1#--prefix=}" ;;
        -h | --help) SHOW_HELP=1 ;;
        *)
            echo "install.sh: unknown option $1 / tuỳ chọn không hợp lệ $1 (--help)" >&2
            exit 2
            ;;
    esac
    shift
done

case "$LANG_ARG" in
    "" | en | vi) ;;
    *)
        echo "install.sh: --lang must be en or vi / --lang phải là en hoặc vi" >&2
        exit 2
        ;;
esac
L=$(detect_lang)

if [ "$SHOW_HELP" = 1 ]; then
    say "Usage: install.sh [--lang en|vi] [--prefix DIR]

Installs vssh to DIR/bin (default: ~/.local/bin). Run it again to update.

  --lang en|vi   language of these messages (default: from VSSH_LANG, then
                 LC_ALL, LC_MESSAGES, LANG; Vietnamese if it starts with vi)
  --prefix DIR   install under DIR instead of ~/.local (or set VSSH_PREFIX)
  --help         show this text" \
        "Cách dùng: install.sh [--lang en|vi] [--prefix THƯ_MỤC]

Cài vssh vào THƯ_MỤC/bin (mặc định: ~/.local/bin). Chạy lại để cập nhật.

  --lang en|vi      ngôn ngữ của các thông báo này (mặc định: theo VSSH_LANG, rồi
                    LC_ALL, LC_MESSAGES, LANG; tiếng Việt nếu bắt đầu bằng vi)
  --prefix THƯ_MỤC  cài vào THƯ_MỤC thay cho ~/.local (hoặc đặt VSSH_PREFIX)
  --help            hiện hướng dẫn này"
    exit 0
fi

[ -n "$PREFIX" ] || die "--prefix needs a directory" "--prefix cần một thư mục"
BIN_DIR="$PREFIX/bin"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Leaves the new binary at $TMP/vssh, or returns 1.
from_checkout() {
    dir=$(dirname "$0")
    grep -qs '^name = "vssh"$' "$dir/crates/vssh-tui/Cargo.toml" || return 1
    command -v cargo >/dev/null ||
        die "This is a source checkout, but cargo was not found. Install Rust: https://rustup.rs" \
            "Đây là thư mục mã nguồn nhưng không tìm thấy cargo. Hãy cài Rust: https://rustup.rs"
    say "Building vssh from this checkout (%s)…" "Đang biên dịch vssh từ thư mục mã nguồn này (%s)…" "$dir"
    (cd "$dir" && cargo build --release --locked)
    cp "$dir/target/release/vssh" "$TMP/vssh"
}

from_release() {
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)
    [ "$arch" = arm64 ] && arch=aarch64
    asset="vssh-$os-$arch.tar.gz"
    url="https://github.com/$REPO/releases/latest/download/$asset"
    say "Looking for a prebuilt binary (%s)…" "Đang tìm bản dựng sẵn (%s)…" "$asset"
    if command -v curl >/dev/null; then
        curl -fsSL "$url" -o "$TMP/$asset" 2>/dev/null || return 1
    elif command -v wget >/dev/null; then
        wget -q "$url" -O "$TMP/$asset" 2>/dev/null || return 1
    else
        return 1
    fi
    tar -xzf "$TMP/$asset" -C "$TMP" vssh 2>/dev/null || return 1
}

from_source() {
    say "No prebuilt binary for this machine; building from source." \
        "Không có bản dựng sẵn cho máy này; sẽ biên dịch từ mã nguồn."
    command -v cargo >/dev/null ||
        die "cargo was not found. Install Rust first: https://rustup.rs" \
            "Không tìm thấy cargo. Hãy cài Rust trước: https://rustup.rs"
    cargo install --git "https://github.com/$REPO" --locked --root "$TMP/cargo" vssh
    cp "$TMP/cargo/bin/vssh" "$TMP/vssh"
}

from_checkout || from_release || from_source

mkdir -p "$BIN_DIR"
chmod 755 "$TMP/vssh"
# Rename into place: this also works while an older vssh is still running.
mv -f "$TMP/vssh" "$BIN_DIR/.vssh.new"
mv -f "$BIN_DIR/.vssh.new" "$BIN_DIR/vssh"

version=$("$BIN_DIR/vssh" --version) ||
    die "Installed %s, but it does not run." "Đã cài %s nhưng không chạy được." "$BIN_DIR/vssh"
say "Installed %s to %s" "Đã cài %s vào %s" "$version" "$BIN_DIR/vssh"

case ":$PATH:" in
    *":$BIN_DIR:"*) say "Start it with: vssh" "Chạy bằng lệnh: vssh" ;;
    *)
        say "%s is not in your PATH. Add this line to your shell profile:" \
            "%s chưa có trong PATH. Hãy thêm dòng này vào file cấu hình shell:" "$BIN_DIR"
        printf '  export PATH="%s:$PATH"\n' "$BIN_DIR"
        ;;
esac
if [ "$L" = vi ]; then
    say "" "Muốn giao diện vssh bằng tiếng Việt: thêm  lang = \"vi\"  vào ~/.config/vssh/config.toml"
fi
