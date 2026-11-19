#!/usr/bin/env bash
# PhpRedis 6.3.0 uses Zend C macro names removed in PHP 8.6.
# Use the pinned compatibility work while the upstream pull request is open.
set -euo pipefail

directory=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/phera-redis-oracle.XXXXXX")
cd "$directory"
git init --quiet
git fetch --quiet --depth 1 https://github.com/phpredis/phpredis.git     70efa6ef076e0b71d7cdbc09a4f36135ebbe0ba7
git checkout --quiet --detach FETCH_HEAD
phpize
CPPFLAGS="${CPPFLAGS:-} -Dphp_hash_bin2hex=zend_bin2hex"     ./configure --with-php-config="$(command -v php-config)"     --enable-redis --disable-redis-igbinary --disable-redis-msgpack     --disable-redis-lzf --disable-redis-zstd --disable-redis-lz4
make -j2
echo "PHERA_PHP_REDIS=$PWD/modules/redis.so" >> "$GITHUB_ENV"
