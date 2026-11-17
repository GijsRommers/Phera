#!/usr/bin/env bash
# Match php -n on installations whose required extensions are shared modules.
set -eo pipefail
real_php=${PHERA_PHP_REAL:-php}
# CI computes this once; local invocations detect compiled-in modules as well.
shared=${PHERA_PHP_SHARED_EXTENSIONS-$("$real_php" -n -r 'foreach (["json", "pcre", "ctype", "pdo", "pdo_pgsql", "pdo_mysql", "mbstring"] as $e) { if (!extension_loaded($e)) echo "$e "; }')}
read -ra modules <<< "$shared"
options=()
for module in "${modules[@]}"; do
    options+=(-d "extension=$module")
done
exec "$real_php" "${options[@]}" "$@"
