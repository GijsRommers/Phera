#!/usr/bin/env bash
# Match php -n on installations whose required extensions are shared modules.
set -eo pipefail
real_php=${PHERA_PHP_REAL:-php}
# CI computes this once; local invocations detect compiled-in modules as well.
shared=${PHERA_PHP_SHARED_EXTENSIONS-$("$real_php" -n -r 'foreach (["json", "pcre", "ctype", "pdo", "pdo_pgsql", "mysqlnd", "pdo_mysql", "pdo_sqlite", "sqlite3", "mbstring", "iconv", "tokenizer", "redis"] as $e) { if (!extension_loaded($e)) echo "$e "; }')}
read -ra modules <<< "$shared"
# Distribution PHP can use the host tzdata even with -n. Load PHP's pinned
# embedded database when its built-in data differs from the date oracle.
tzdb=2026.5
if [[ "${PHERA_TEST_PHP:-8.5}" == 8.6 ]]; then
    tzdb=2026.6
fi
if [[ " $shared " != *" timezonedb "* ]] &&
    [[ $("$real_php" -n -r 'echo timezone_version_get();') != "$tzdb" ]]; then
    modules+=(timezonedb)
fi
options=()
# Homebrew can configure shared extensions outside PHP's compiled default directory.
extension_dir=$("$real_php" -r 'echo ini_get("extension_dir");')
options=(-d "extension_dir=$extension_dir")
for module in "${modules[@]}"; do
    if [[ "$module" == timezonedb && -n "${PHERA_PHP_TIMEZONEDB:-}" ]]; then
        module=$PHERA_PHP_TIMEZONEDB
    fi
    options+=(-d "extension=$module")
done
# Some PDO probes omit -n; keep those on the same ini-free oracle too.
exec "$real_php" -n "${options[@]}" "$@"
