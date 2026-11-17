# Redis implementation report

Branch: `ext/redis`. Pull request: <https://github.com/GijsRommers/Phera/pull/13>.
The overall compatibility gate remains **PARTIAL**; the PR must stay a draft.

## Implementation

`Redis` and `RedisException` are registered. There are no new global PHP functions.
The complete reflection-derived inventory is [METHODS.md](METHODS.md): every one of
phpredis 6.3.0's 287 Redis methods has DONE, PARTIAL or NOT DONE status. Signature
registration for an unsupported method does not count as implementing it.
RedisCluster, RedisSentinel and RedisArray are not registered.

The client is `redis` **1.0.5**, selected for its pure Rust asynchronous Tokio client,
RESP2/RESP3 support, binary-safe command API, Unix transport and rustls integration.
`rustls-rustcrypto` **0.1.0-pre.0** supplies the pure Rust TLS crypto provider, avoiding
an OpenSSL, AWS-LC or ring dependency in the active Redis dependency graph. The
provider's prerelease status is a dependency risk. Tokio provides async sockets,
semaphores, channels and the dedicated runtime. No unsafe Rust was added.

Every PHP Redis object keeps its request-local state in `Object::native`. Owned
command bytes and RESP replies cross to a small process-lifetime I/O runtime; PHP
`Rc` values and callbacks stay on the worker. Pool admission, commands, blocking pops
and subscription waits yield asynchronously. Subscriptions have dedicated connections
that never return to the pool. Both connect and pconnect borrow exclusive pooled
connections; persistent IDs partition the pools.

The first implementation commit, `badb358`, extracts `phera-io-pool`. PDO and Redis
share the process runtime host, capacity permits, idle FIFO, expiry and reset/close
accounting. Protocol cleanup stays in each extension's adapter. Redis RESET clears
transactions, watches, selected database, authentication, client names and subscriptions.
An in-flight I/O task owns its lease even if the PHP request is cancelled. Failed
cleanup retires the socket. PDO's settings and protocol operations remain in PDO.

The engine adds union-argument forms and a general native-method preparation hook.
The hook preserves reflected signatures while allowing extension-specific C-parser
coercion, arity and deprecation behavior; it contains no Redis-specific VM logic.

## Verification

Oracle: **PHP 8.5.12, phpredis 6.3.0, Redis 8.4.0**. Local `php -n` lacks Redis;
`.github/scripts/php-oracle.sh` loads it explicitly. CI builds phpredis 6.3.0 with
optional serializers/codecs disabled, loads Redis through
`PHERA_PHP_SHARED_EXTENSIONS`, and checks it in `check-prerequisites.php`.
CI includes the health-checked Redis service and disables persistence before testing.

The final **192-script** release run matched **2,014 instrumented calls with zero
mismatches**. Its 500-request concurrency proof completed in **409.9 ms**. The fixture
count excludes the separate Unix, blocking-pop and six pub/sub comparisons.
Comparisons use the same PHP files through real PHP and the release Phera executable,
checking stdout, stderr and successful exit status. Mismatches fail the suite.
The suite covers missing and WRONGTYPE keys, NUL bytes, 64 KiB values, NONE/JSON,
prefixes, field expiry, streams, mixed-family transactions/pipelines, WATCH abort,
metadata reads, invalid signatures and several phpredis parser quirks.

Four native pool tests cover reuse/RESET after MULTI/WATCH/database/client-name changes,
a subscribed socket, capacity limits, CLIENT KILL replacement and use after the worker
that opened a connection has stopped. Separate PHP comparisons cover Unix transport,
a blocking pop satisfied by another connection, and subscribe/psubscribe/ssubscribe
with callbacks, callback-initiated unsubscribe and a second-process publisher.
Subscription comparisons include both empty and configured key prefixes. Local workspace
Clippy with warnings denied, formatting, the unsafe-token scanner, and the affected
extension API/VM/runtime unit suites pass.
The HTTP proof sends 500 simultaneous 300 ms BLPOP requests through `serve --workers 1`.

PDO's six non-database unit tests passed locally. Hosted CI at `a9166b0` passed all
PDO unit/integration tests and its release-binary PHP comparisons with the shared
pool crate. No other local container or database was used. The same hosted run passed
Redis's pool, blocking-pop, concurrency, pub/sub and Unix tests; its older AUTH
comparison failure is fixed by the subsequent implementation changes.

## Remaining limits

- The 287-method inventory explicitly distinguishes implemented, partially verified
  and unimplemented methods. The requested every-calling-form/every-option cross-product
  is not complete, even when the checked-in comparisons all pass.
- The 17 unimplemented methods are failover, migrate, transferred-byte counters and
  the 13 vector-set methods. They throw explicit not-implemented exceptions.
- Constructor/connect SSL contexts, custom trust roots/client certificates, keepalive,
  user-configurable retry/backoff and related options are incomplete. TLS uses WebPKI
  roots and verifies certificates/hostnames, but an end-to-end TLS fixture is not present.
  Unix sockets are verified. The limited handshake retry never replays PHP commands.
- SERIALIZER_NONE and SERIALIZER_JSON work. Phera has no registered `serialize` or
  `unserialize`, so SERIALIZER_PHP is an engine gap. igbinary/msgpack are not implemented
  or enabled in CI. Compression supports NONE only; LZF, ZSTD and LZ4 are missing.
- Advanced server subcommands, topology changes and all callback error/reconnect cases
  have not received exhaustive oracle coverage. FLUSHALL is never run. FLUSHDB tests
  use only database 12 in the task-owned container.
- phpredis 6.3.0 exits 255 with truncated output when var_dump prints a WITHSCORES map
  whose JSON-deserialized member is an array. The exact manual reproducer is preserved
  in `tests/oracle-limitations/json-array-score-key.php`. It is not counted as a passing
  comparison. JSON arrays are compared in the other collection shapes; score maps use
  valid string keys. Phera does not emulate invalid native memory behavior.

The sole task container `phera-test-redis` is left running at localhost:16399.
Its exact start command and test commands are in [tests/README.md](tests/README.md).
