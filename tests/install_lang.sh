#!/bin/sh
# Language selection of install.sh, observed through its --help text (which
# has no side effects). Run by `cargo test` via tests/install.rs.
set -eu
cd "$(dirname "$0")/.."

EN="Usage: install.sh"
VI="Cách dùng: install.sh"
fail=0

# expect TEXT VAR=value... -- installer arguments...
expect() {
    want=$1
    shift
    envs=""
    while [ "$1" != "--" ]; do
        envs="$envs $1"
        shift
    done
    shift
    # shellcheck disable=SC2086  # $envs is a list of VAR=value words
    out=$(env -i PATH=/usr/bin:/bin HOME=/nonexistent $envs sh install.sh "$@" 2>&1) || true
    case "$out" in
        *"$want"*) ;;
        *)
            echo "FAIL:$envs install.sh $*: expected '$want', got: $(echo "$out" | head -1)"
            fail=1
            ;;
    esac
}

# Nothing set: English.
expect "$EN" -- --help
# $LANG decides when nothing more specific is set.
expect "$VI" LANG=vi_VN.UTF-8 -- --help
expect "$VI" LANG=vi -- --help
expect "$EN" LANG=en_US.UTF-8 -- --help
expect "$EN" LANG=C -- --help
# LC_ALL beats LC_MESSAGES beats LANG, in both directions.
expect "$VI" LANG=en_US.UTF-8 LC_ALL=vi_VN.UTF-8 -- --help
expect "$EN" LANG=vi_VN.UTF-8 LC_ALL=C -- --help
expect "$VI" LANG=en_US.UTF-8 LC_MESSAGES=vi_VN -- --help
expect "$EN" LANG=vi_VN LC_MESSAGES=vi_VN LC_ALL=en_GB.UTF-8 -- --help
# VSSH_LANG beats the locale.
expect "$VI" LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 VSSH_LANG=vi -- --help
expect "$EN" LANG=vi_VN.UTF-8 VSSH_LANG=en -- --help
# --lang beats everything, in both spellings.
expect "$VI" LANG=en_US.UTF-8 VSSH_LANG=en -- --lang vi --help
expect "$EN" LANG=vi_VN.UTF-8 VSSH_LANG=vi -- --lang=en --help
expect "$VI" -- --help --lang vi
# Bad input is refused, in both languages, before anything is installed.
expect "--lang must be en or vi / --lang phải là en hoặc vi" -- --lang fr
expect "unknown option --frobnicate" -- --frobnicate
for bad in "--lang fr" "--frobnicate" "--lang"; do
    # shellcheck disable=SC2086
    if env -i PATH=/usr/bin:/bin HOME=/nonexistent sh install.sh $bad --help >/dev/null 2>&1; then
        echo "FAIL: install.sh $bad should exit non-zero"
        fail=1
    fi
done

[ "$fail" = 0 ] && echo "install.sh language selection: ok"
exit "$fail"
