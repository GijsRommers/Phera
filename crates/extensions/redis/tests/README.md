# Redis oracle and pool tests

The oracle used here is PHP 8.5.12 with phpredis **6.3.0** and Redis **8.4.0**.
The complete reflected signatures and constants are in `../tools/phpredis-6.3.0.json`.

The task's only container was started with this exact command and is left running:

```sh
docker run -d --name phera-test-redis -p 127.0.0.1:16399:6379 redis:8.4.0 redis-server --save '' --appendonly no
```

Do not use FLUSHALL. Scripts select database 14 and receive a unique key prefix
from the test runner. They delete only their own prefixed keys. The HTTP test uses
database 13 and its own key per request. ACL tests create and remove their own user.

```sh
cargo build --release -p phera-cli
PHERA_PHP="$PWD/.github/scripts/php-oracle.sh" \
PHERA_BIN="$PWD/target/release/phera" cargo test -p phera-ext-redis -- --nocapture
cargo clippy -p phera-ext-redis -p phera-io-pool -p phera-ext-pdo --all-targets -- -D warnings
```

`php -n` does not include Redis locally. The shared CI/local wrapper detects missing
modules and loads Redis explicitly through `PHERA_PHP_SHARED_EXTENSIONS`; it preserves
the configured extension directory for Homebrew's separate PECL directory. The oracle
is resolved with `phera_extension_api::testing::oracle::find_php`, never hard-coded.

The oracle test compares stdout, stderr and successful exit status for the identical
PHP script through real PHP and the release executable. Artifacts go to
`target/redis-oracle`. `CALLS=` counts compared calls, not setup/cleanup commands.
Every mismatch fails; no cases are marked expected failures. Run only one oracle
suite at a time because saved artifact filenames are fixed.

The release-only tests require PHERA_BIN and run in CI's end-to-end job. Native pool
unit tests always run and fail if the Redis service is unavailable. Pub/sub and blocking
pop comparisons launch a second real-PHP publisher process. The concurrency test runs
500 simultaneous HTTP requests, each doing a 300 ms BLPOP on its own key, with one
Phera worker and a 512-connection pool. It requires all responses to succeed and total
time below two seconds.
