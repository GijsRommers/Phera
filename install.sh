#!/bin/sh
# Install a published Phera binary.
set -eu

fail() {
    printf 'Phera installer: %s\n' "$*" >&2
    exit 1
}

[ "$#" -le 1 ] || fail 'Usage: sh install.sh [version]'
case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux)
        getconf GNU_LIBC_VERSION >/dev/null 2>&1 || fail 'Linux releases require glibc.'
        os=linux ;;
    *) fail "Unsupported operating system: $(uname -s)" ;;
esac
case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64) arch=x86_64 ;;
    *) fail "Unsupported architecture: $(uname -m)" ;;
esac

base=${PHERA_RELEASE_BASE_URL:-https://github.com/GijsRommers/Phera/releases}
base=${base%/}
latest=https://api.github.com/repos/GijsRommers/Phera/releases/latest
if [ -n "${PHERA_RELEASE_BASE_URL:-}" ]; then
    latest=$base/latest
fi

download() {
    case "$1" in
        file://*) cp "${1#file://}" "$2" ;;
        https://*|http://*)
            if command -v curl >/dev/null 2>&1; then
                curl -q -fsSL -o "$2" "$1"
            elif command -v wget >/dev/null 2>&1; then
                wget -q -O "$2" "$1"
            else
                fail 'Install curl or wget to download Phera.'
            fi ;;
        *) fail 'Use an HTTPS, HTTP or file:// release base URL.' ;;
    esac
}

bin_dir=${PHERA_INSTALL:-$HOME/.phera}/bin
case "$bin_dir" in
    /*) ;;
    *) bin_dir=$PWD/$bin_dir ;;
esac
mkdir -p "$bin_dir"
[ ! -d "$bin_dir/phera" ] || fail "$bin_dir/phera is a directory."
work=$(mktemp -d "$bin_dir/.phera-install.XXXXXXXX")
trap 'rm -rf "$work"' 0
trap 'exit 1' HUP INT TERM

version=${1:-${PHERA_VERSION:-}}
if [ -z "$version" ]; then
    download "$latest" "$work/latest" || fail 'Could not find the latest release.'
    version=$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$work/latest")
fi
version=${version#v}
case "$version" in
    ''|*[!0-9A-Za-z.+-]*) fail 'Expected a version like v0.1.0.' ;;
esac
printf '%s\n' "$version" | LC_ALL=C grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$' ||
    fail 'Expected a version like v0.1.0.'

archive=phera-$version-$os-$arch.tar.gz
release=$base/download/v$version
download "$release/SHA256SUMS" "$work/SHA256SUMS" || fail "Could not download checksums for v$version."
download "$release/$archive" "$work/$archive" || fail "Could not download $archive."

expected=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print tolower($1) }' "$work/SHA256SUMS")
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum < "$work/$archive")
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 < "$work/$archive")
else
    fail 'Install sha256sum or shasum to verify the download.'
fi
[ "$expected" = "${actual%% *}" ] || fail "SHA-256 verification failed for $archive."

tar -xOzf "$work/$archive" phera > "$work/phera" || fail "Could not extract $archive."
[ -s "$work/phera" ] || fail "No Phera binary in $archive."
chmod 755 "$work/phera"
mv -f "$work/phera" "$bin_dir/phera"
printf 'Installed Phera %s in %s\n' "$version" "$bin_dir"

case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *)
        quoted_dir=$(printf '%s' "$bin_dir" | sed "s/'/'\\\\''/g")
        printf 'Add this line to your shell profile:\n'
        printf "export PATH='%s':\"\$PATH\"\n" "$quoted_dir" ;;
esac
