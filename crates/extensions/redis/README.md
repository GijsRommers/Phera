# Redis in Phera

```php
$r = new Redis(['host' => '127.0.0.1', 'port' => 6379]);
$r->set('key', 'value');
$value = $r->get('key');
```

The extension uses `redis` 1.0.5 with Tokio, RESP2 commands and RESP3 subscription
pushes. It is a pure Rust client with asynchronous sockets and binary-safe arguments.
`tls://` uses rustls with the RustCrypto provider (currently a prerelease dependency),
WebPKI roots and certificate/hostname validation. Custom PHP SSL context options are
not implemented. Unix paths, `unix://` and `file://` use Tokio Unix sockets.

Every Redis object exclusively borrows from a process-wide pool. `connect` and
`pconnect` both pool; persistent identifiers partition the pool. PHP values remain
on their worker; only owned command bytes and RESP replies cross to dedicated I/O
threads. Constructors configure lazy connections. Neither network waits nor pool
admission blocks the PHP worker thread.

`phera-io-pool` is shared with PDO: runtime construction, FIFO capacity permits,
idle storage, idle/lifetime expiry, and asynchronous reset/close accounting. PDO's
protocol reset and Redis's RESET are separate adapters. PDO keeps its existing
settings and database-specific lease operations.

Returning a Redis connection sends RESET, clearing MULTI, WATCH, authentication,
selected database, client name and subscriptions. Failed reset discards the socket.
A lease stays alive while its in-flight command completes, including cancellation.
Subscriptions use separate sockets that never enter the pool. Callbacks run on the
PHP worker while the subscription future waits, and can unsubscribe.

| Environment setting | Default |
| --- | ---: |
| `PHERA_REDIS_POOL_SIZE` | 512 |
| `PHERA_REDIS_POOL_IDLE_SECONDS` | 60 |
| `PHERA_REDIS_POOL_LIFETIME_SECONDS` | 1800 |
| `PHERA_REDIS_POOL_WAIT_SECONDS` | 30 |
| `PHERA_REDIS_IO_THREADS` | 2 |

Settings are read once on first Redis use. Limits apply per endpoint/persistent ID.
An exhausted pool waits asynchronously, then raises RedisException.

Serialization supports NONE and JSON. Phera does not register `serialize` or
`unserialize`, so SERIALIZER_PHP is not implemented. igbinary and msgpack are not
implemented. Compression supports NONE; LZF, ZSTD and LZ4 are not implemented.
RedisCluster, RedisSentinel and RedisArray are deliberately not registered.

See [tests/README.md](tests/README.md) for reproducible validation and
[METHODS.md](METHODS.md) for the reflection-derived status of all 287 methods.
See [REPORT.md](REPORT.md) for evidence and remaining gaps. The crate remains a work in
progress until the full requested compatibility gate passes.
