#!/usr/bin/env bash
# Match php -n on installations whose required extensions are shared modules.
set -eo pipefail
real_php=${PHERA_PHP_REAL:-php}
# CI computes this once; local invocations detect compiled-in modules as well.
shared=${PHERA_PHP_SHARED_EXTENSIONS-$("$real_php" -n -r 'foreach (["json", "pcre", "ctype", "pdo", "pdo_pgsql", "mysqlnd", "pdo_mysql", "mbstring", "iconv", "tokenizer"] as $e) { if (!extension_loaded($e)) echo "$e "; }')}
read -ra modules <<< "$shared"
options=()
for module in "${modules[@]}"; do
    options+=(-d "extension=$module")
done
# Some PDO probes omit -n; keep those on the same ini-free oracle too.
exec "$real_php" -n "${options[@]}" "$@"
