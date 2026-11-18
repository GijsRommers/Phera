#!/usr/bin/env bash
# Use the same slim timezone database as the official PHP build.
set -euo pipefail

php_binary=$(command -v php)
if [[ $("$php_binary" -n -r 'echo timezone_version_get();') == 2026.5 ]]; then
    exit 0
fi

directory=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/phera-date-oracle.XXXXXX")
cd "$directory"
curl --fail --silent --show-error --location https://pecl.php.net/get/timezonedb-2026.5.tgz \
    --output timezonedb.tgz
echo 'affcad3175b7d2d3dde359ca2737addc8c8c95de355c2c73a2bab69bbbc2e012  timezonedb.tgz' \
    | sha256sum --check
tar -xzf timezonedb.tgz
cd timezonedb-2026.5
phpize
CPPFLAGS="${CPPFLAGS:-} -DTIMELIB_SUPPORT_SLIM_FILE" \
    ./configure --with-php-config="$(command -v php-config)"
make -j2
echo "PHERA_PHP_TIMEZONEDB=$PWD/modules/timezonedb.so" >> "$GITHUB_ENV"
