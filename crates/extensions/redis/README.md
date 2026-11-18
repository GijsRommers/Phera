# Redis in Phera

The `Redis` class of phpredis 6.3.0, and `RedisException`, without a C
extension:

```php
$redis = new Redis(['host' => '127.0.0.1', 'port' => 6379]);
$redis->set('greeting', 'hello', ['ex' => 60]);
echo $redis->get('greeting');
```

It stands on the `redis` crate (1.0.5) with Tokio: RESP2 for commands, RESP3
for subscriptions. `tls://` hosts use rustls with the RustCrypto provider (a
pre-release crate) and WebPKI roots, verifying certificates and host names.
Unix sockets work as a path, `unix://` or `file://`.

## How it runs

Everything that waits is asynchronous: connecting, every command, blocking
pops, waiting for a pool slot, a subscription. While a request waits, its
worker thread runs other requests.

Every `Redis` object borrows a socket from a process-wide pool, shared by all
workers (`crates/io-pool` describes how a command travels to the I/O threads
and back). `connect` and `pconnect` both borrow; a persistent id has sockets
of its own. When the object is gone, the socket gets a RESET (which ends
MULTI and WATCH, logs out, selects database 0, clears the client name and
subscriptions) and goes back to the pool; a socket whose RESET fails is
closed. A subscription uses a socket of its own that never enters the pool;
its PHP callback runs on the worker and may unsubscribe.

| Environment setting | Default | |
| --- | ---: | --- |
| `PHERA_REDIS_POOL_SIZE` | 512 | sockets per endpoint and persistent id |
| `PHERA_REDIS_POOL_IDLE_SECONDS` | 60 | how long a returned socket may stay idle |
| `PHERA_REDIS_POOL_LIFETIME_SECONDS` | 1800 | how old a socket may get |
| `PHERA_REDIS_POOL_WAIT_SECONDS` | 30 | how long `connect` waits when all are busy, then throws |
| `PHERA_REDIS_IO_THREADS` | 2 | threads of the Redis I/O runtime |

They are read once, on first use; a script cannot change them.

## What is not there

- RedisCluster, RedisSentinel and RedisArray are not registered.
- Serializers NONE and JSON only (Phera has no `serialize()` yet, so not
  SERIALIZER_PHP; no igbinary or msgpack). Compression NONE only (no LZF,
  ZSTD, LZ4).
- No SSL context options, custom trust roots or client certificates. The
  `retryInterval`, `maxRetries` and `backoff` options are checked, not used.
  No test connects over TLS.
- 17 methods are declared but throw "not implemented yet": `failover`,
  `migrate`, `getTransferredBytes`, `clearTransferredBytes` and the 13
  vector-set methods (`vadd` ...). They stay declared so that calling them
  with wrong arguments fails as in phpredis.
- `reset()` in pipeline mode is queued; phpredis raises a fatal error.
- phpredis 6.3.0 itself crashes when `var_dump` prints a WITHSCORES map
  whose JSON-decoded member is an array; `tests/oracle-limitations/` keeps
  the reproducer, which is not part of the comparison.

## Tests

The comparison scripts run against a Redis 8.4 server of their own:

```sh
docker run -d --name phera-test-redis -p 127.0.0.1:16399:6379 redis:8.4.0 \
    redis-server --save '' --appendonly no
cargo build --release -p phera-cli
PHERA_PHP="$PWD/.github/scripts/php-oracle.sh" PHERA_BIN="$PWD/target/release/phera" \
    cargo test -p phera-ext-redis -- --nocapture
```

The oracle is PHP 8.5 with phpredis 6.3.0; locally `php -n` has no Redis,
and `.github/scripts/php-oracle.sh` loads it. Each script in `tests/scripts`
runs through PHP and through Phera, which must succeed with the same stdout
and stderr; the script prints `CALLS=n`, the number of calls it compared.
Scripts 000 to 171 are written by `tools/generate-tests.py` from its call
tables (present, missing and wrong-type keys, NUL bytes, 64 KiB values,
prefixes, serializers); 172 onwards are written by hand. Without `PHERA_BIN`
only the pool tests run.

The scripts use database 14 and a key prefix of their own, and delete their
keys; 190 uses database 12 for FLUSHDB, 193 databases 10 and 11, the HTTP
test database 13. Nothing runs FLUSHALL. Besides the scripts there are the
pool tests (reuse after MULTI, WATCH, SELECT, a subscription; limits; a
killed connection; a socket that outlives its worker), subscriptions with a
publisher in a second PHP process, a blocking pop satisfied from another
process, a Unix socket, and 500 overlapping 300 ms BLPOP requests through
`phera serve --workers 1`, which must finish within two seconds.

## Methods

What the comparison scripts call, as `tools/inventory.py` finds it.

<!-- inventory: tools/inventory.py -->
266 compared, 1 with only their argument errors compared, 3 not compared, 17 not implemented.

| Method | Status | Compared in, and notes |
| --- | --- | --- |
| `__construct` | compared | `171_watch`, `176_connection_options`, `182_auth` and 8 more The `ssl`, `backoff`, `retryInterval` and `maxRetries` options are checked, not used. |
| `__destruct` | compared | `192_destructor` |
| `_compress` | compared | `193_main_forms` Serializers NONE and JSON only; compression NONE only. |
| `_uncompress` | compared | `193_main_forms` Serializers NONE and JSON only; compression NONE only. |
| `_prefix` | compared | `166_options` |
| `_serialize` | compared | `193_main_forms` Serializers NONE and JSON only; compression NONE only. |
| `_unserialize` | compared | `193_main_forms` Serializers NONE and JSON only; compression NONE only. |
| `_pack` | compared | `193_main_forms` Serializers NONE and JSON only; compression NONE only. |
| `_unpack` | compared | `193_main_forms` Serializers NONE and JSON only; compression NONE only. |
| `acl` | compared | `185_auth_forms`, `193_main_forms` |
| `append` | compared | `004_append` |
| `auth` | compared | `182_auth`, `185_auth_forms` |
| `bgSave` | not compared | Not run: it would change the shared test server. |
| `bgrewriteaof` | not compared | Not run: it would change the shared test server. |
| `waitaof` | compared | `193_main_forms` |
| `bitcount` | compared | `010_bitcount`, `025_bit_ranges` |
| `bitop` | compared | `026_bitop` |
| `bitpos` | compared | `011_bitpos`, `025_bit_ranges` |
| `blPop` | compared | `089_blpop`, `179_option_modes`, `tests/blocking.rs` and 1 more |
| `brPop` | compared | `090_brpop` |
| `brpoplpush` | compared | `088_brpoplpush` |
| `bzPopMax` | compared | `135_bzpopmax` |
| `bzPopMin` | compared | `134_bzpopmin` |
| `bzmpop` | compared | `133_bzmpop` |
| `zmpop` | compared | `132_zmpop` |
| `blmpop` | compared | `092_blmpop` |
| `lmpop` | compared | `091_lmpop` |
| `clearLastError` | compared | `000_get`, `001_getdel`, `002_getex` and 101 more |
| `client` | compared | `174_server` |
| `close` | compared | `176_connection_options`, `182_auth`, `185_auth_forms` and 2 more |
| `command` | compared | `174_server` |
| `config` | compared | `174_server` |
| `connect` | compared | `171_watch`, `176_connection_options`, `182_auth` and 7 more TCP, Unix sockets and `tls://` (rustls); no SSL context options. |
| `copy` | compared | `035_copy`, `181_prefix` |
| `dbSize` | compared | `174_server`, `190_flushdb` |
| `debug` | compared | `193_main_forms` |
| `decr` | compared | `015_decr` |
| `decrBy` | compared | `016_decrby` |
| `del` | compared | `017_set`, `018_setex`, `019_psetex` and 33 more |
| `delifeq` | compared | `193_main_forms` A Valkey command; Redis 8.4 answers with an error, compared as such. |
| `delete` | compared | `191_functions`, `193_main_forms` |
| `discard` | compared | `183_transaction_state`, `support` |
| `dump` | compared | `036_dump_restore` |
| `echo` | compared | `165_connection`, `191_functions` |
| `eval` | compared | `162_eval`, `181_prefix`, `188_mixed_batches` |
| `eval_ro` | compared | `193_main_forms` |
| `evalsha` | compared | `163_evalsha` |
| `evalsha_ro` | compared | `193_main_forms` |
| `exec` | compared | `169_pipeline`, `170_multi`, `171_watch` and 9 more |
| `exists` | compared | `029_exists`, `188_mixed_batches`, `193_main_forms` |
| `expire` | compared | `037_expire` |
| `expireAt` | compared | `039_expireat`, `041_ttl`, `042_pttl` and 2 more |
| `failover` | not implemented | Throws "Redis::failover is not implemented yet". |
| `expiretime` | compared | `043_expiretime` |
| `pexpiretime` | compared | `044_pexpiretime` |
| `fcall` | compared | `191_functions` |
| `fcall_ro` | compared | `191_functions` |
| `flushAll` | arguments only | Only its argument errors are compared. Not run: it would change the shared test server. |
| `flushDB` | compared | `190_flushdb` Only on database 12, which belongs to its script. |
| `function` | compared | `191_functions` |
| `geoadd` | compared | `155_geoadd`, `156_geodist`, `157_geohash` and 6 more |
| `geodist` | compared | `156_geodist` |
| `geohash` | compared | `157_geohash`, `188_mixed_batches` |
| `geopos` | compared | `158_geopos` |
| `georadius` | compared | `159_georadius` |
| `georadius_ro` | compared | `193_main_forms` |
| `georadiusbymember` | compared | `193_main_forms` |
| `georadiusbymember_ro` | compared | `193_main_forms` |
| `geosearch` | compared | `160_geosearch` |
| `geosearchstore` | compared | `161_geosearchstore` |
| `get` | compared | `000_get`, `017_set`, `018_setex` and 32 more |
| `getWithMeta` | compared | `184_metadata` |
| `getAuth` | compared | `165_connection`, `182_auth`, `185_auth_forms` |
| `getBit` | compared | `008_getbit` |
| `getEx` | compared | `002_getex` |
| `getDBNum` | compared | `165_connection`, `176_connection_options`, `183_transaction_state` and 2 more |
| `getDel` | compared | `001_getdel` |
| `getHost` | compared | `165_connection`, `176_connection_options` |
| `getLastError` | compared | `000_get`, `001_getdel`, `002_getex` and 101 more |
| `getMode` | compared | `183_transaction_state`, `187_transaction_errors`, `192_destructor` and 1 more |
| `getOption` | compared | `166_options`, `179_option_modes` Retry, backoff and keepalive options are not kept. |
| `getPersistentID` | compared | `165_connection`, `176_connection_options` |
| `getPort` | compared | `165_connection`, `176_connection_options`, `tests/unix.rs` |
| `serverName` | compared | `176_connection_options` |
| `serverVersion` | compared | `176_connection_options` |
| `getRange` | compared | `006_getrange` |
| `lcs` | compared | `193_main_forms`, `194_lcs_options` |
| `getReadTimeout` | compared | `165_connection`, `176_connection_options` |
| `getset` | compared | `003_getset` |
| `getTimeout` | compared | `165_connection`, `176_connection_options` |
| `getTransferredBytes` | not implemented | Throws "Redis::gettransferredbytes is not implemented yet". |
| `clearTransferredBytes` | not implemented | Throws "Redis::cleartransferredbytes is not implemented yet". |
| `hDel` | compared | `053_hdel` |
| `hExists` | compared | `051_hexists` |
| `hGet` | compared | `045_hget`, `167_serializer_none`, `168_serializer_json` and 2 more |
| `hGetAll` | compared | `046_hgetall`, `169_pipeline`, `170_multi` and 3 more |
| `hGetWithMeta` | compared | `184_metadata` |
| `hIncrBy` | compared | `057_hincrby` |
| `hIncrByFloat` | compared | `058_hincrbyfloat` |
| `hKeys` | compared | `047_hkeys` |
| `hLen` | compared | `049_hlen` |
| `hMget` | compared | `052_hmget`, `189_json_collections` |
| `hgetex` | compared | `061_hgetex` |
| `hsetex` | compared | `177_hsetex` |
| `hgetdel` | compared | `060_hgetdel` |
| `hMset` | compared | `055_hmset`, `173_scans`, `189_json_collections` |
| `hRandField` | compared | `059_hrandfield` |
| `hSet` | compared | `000_get`, `001_getdel`, `002_getex` and 48 more |
| `hSetNx` | compared | `056_hsetnx` |
| `hStrLen` | compared | `050_hstrlen` |
| `hVals` | compared | `048_hvals` |
| `hexpire` | compared | `062_hexpire` |
| `hpexpire` | compared | `063_hpexpire` |
| `hexpireat` | compared | `064_hexpireat` |
| `hpexpireat` | compared | `065_hpexpireat`, `066_httl`, `067_hpttl` and 3 more |
| `httl` | compared | `066_httl` |
| `hpttl` | compared | `067_hpttl` |
| `hexpiretime` | compared | `068_hexpiretime` |
| `hpexpiretime` | compared | `069_hpexpiretime` |
| `hpersist` | compared | `070_hpersist` |
| `hscan` | compared | `173_scans`, `193_main_forms` |
| `expiremember` | compared | `193_main_forms` A KeyDB command; Redis answers with an error, compared as such. |
| `expirememberat` | compared | `193_main_forms` A KeyDB command; Redis answers with an error, compared as such. |
| `incr` | compared | `012_incr` |
| `incrBy` | compared | `013_incrby` |
| `incrByFloat` | compared | `014_incrbyfloat` |
| `info` | compared | `174_server` |
| `isConnected` | compared | `165_connection`, `176_connection_options`, `192_destructor` and 1 more |
| `keys` | compared | `193_main_forms`, `support` |
| `lInsert` | compared | `083_linsert` |
| `lLen` | compared | `071_llen` |
| `lMove` | compared | `085_lmove` |
| `blmove` | compared | `087_blmove` |
| `lPop` | compared | `074_lpop`, `169_pipeline`, `170_multi` and 5 more |
| `lPos` | compared | `084_lpos` |
| `lPush` | compared | `076_lpush` |
| `rPush` | compared | `071_llen`, `072_lrange`, `073_lindex` and 28 more |
| `lPushx` | compared | `078_lpushx` |
| `rPushx` | compared | `079_rpushx` |
| `lSet` | compared | `080_lset` |
| `lastSave` | compared | `193_main_forms` |
| `lindex` | compared | `073_lindex`, `189_json_collections` |
| `lrange` | compared | `072_lrange`, `189_json_collections` |
| `lrem` | compared | `081_lrem` |
| `ltrim` | compared | `082_ltrim`, `193_main_forms` |
| `mget` | compared | `021_mget`, `181_prefix` |
| `migrate` | not implemented | Throws "Redis::migrate is not implemented yet". |
| `move` | compared | `193_main_forms` |
| `mset` | compared | `022_mset`, `181_prefix`, `193_main_forms` and 1 more |
| `msetnx` | compared | `023_msetnx` |
| `multi` | compared | `170_multi`, `171_watch`, `181_prefix` and 9 more |
| `object` | compared | `174_server` |
| `open` | compared | `176_connection_options` |
| `pconnect` | compared | `176_connection_options` Pooled like connect; the persistent id gets sockets of its own. |
| `persist` | compared | `031_persist` |
| `pexpire` | compared | `038_pexpire` |
| `pexpireAt` | compared | `040_pexpireat` |
| `pfadd` | compared | `144_pfadd`, `145_pfcount`, `146_pfmerge` and 1 more |
| `pfcount` | compared | `144_pfadd`, `145_pfcount`, `146_pfmerge` and 1 more |
| `pfmerge` | compared | `146_pfmerge` |
| `ping` | compared | `165_connection`, `176_connection_options`, `179_option_modes` and 6 more |
| `pipeline` | compared | `169_pipeline`, `183_transaction_state`, `184_metadata` and 6 more |
| `popen` | compared | `176_connection_options` |
| `psetex` | compared | `019_psetex` |
| `psubscribe` | compared | `tests/pubsub.rs` |
| `pttl` | compared | `042_pttl` |
| `publish` | compared | `188_mixed_batches`, `tests/pubsub.rs` |
| `pubsub` | compared | `193_main_forms`, `195_pubsub_numsub` |
| `punsubscribe` | compared | `tests/pubsub.rs` |
| `rPop` | compared | `075_rpop`, `186_zero_counts` |
| `randomKey` | compared | `193_main_forms` |
| `rawcommand` | compared | `164_rawcommand`, `174_server`, `179_option_modes` and 5 more |
| `rename` | compared | `033_rename` |
| `renameNx` | compared | `034_renamenx` |
| `reset` | compared | `196_reset` In pipeline mode phpredis raises a fatal error; Phera queues the command. |
| `restore` | compared | `036_dump_restore` |
| `role` | compared | `193_main_forms` |
| `rpoplpush` | compared | `086_rpoplpush` |
| `sAdd` | compared | `093_sadd`, `094_saddarray`, `095_scard` and 22 more |
| `sAddArray` | compared | `094_saddarray` |
| `sDiff` | compared | `103_sdiff` |
| `sDiffStore` | compared | `106_sdiffstore` |
| `sInter` | compared | `104_sinter` |
| `sintercard` | compared | `109_sintercard` |
| `sInterStore` | compared | `107_sinterstore` |
| `sMembers` | compared | `101_smembers`, `181_prefix`, `188_mixed_batches` and 1 more |
| `sMisMember` | compared | `097_smismember`, `189_json_collections` |
| `sMove` | compared | `102_smove` |
| `sPop` | compared | `099_spop`, `186_zero_counts`, `189_json_collections` |
| `sRandMember` | compared | `100_srandmember`, `186_zero_counts` |
| `sUnion` | compared | `105_sunion` |
| `sUnionStore` | compared | `108_sunionstore` |
| `save` | not compared | Not run: it would change the shared test server. |
| `scan` | compared | `173_scans` |
| `scard` | compared | `095_scard` |
| `script` | compared | `163_evalsha`, `188_mixed_batches`, `193_main_forms` |
| `select` | compared | `171_watch`, `176_connection_options`, `182_auth` and 9 more |
| `set` | compared | `000_get`, `001_getdel`, `002_getex` and 134 more |
| `setBit` | compared | `009_setbit` |
| `setRange` | compared | `007_setrange` |
| `setOption` | compared | `166_options`, `167_serializer_none`, `168_serializer_json` and 10 more Serializers NONE and JSON only; no compression. |
| `setex` | compared | `018_setex` |
| `setnx` | compared | `020_setnx` |
| `sismember` | compared | `096_sismember`, `189_json_collections` |
| `slaveof` | compared | `193_main_forms` |
| `replicaof` | compared | `193_main_forms` |
| `touch` | compared | `030_touch` |
| `slowlog` | compared | `174_server` |
| `sort` | compared | `178_sort` |
| `sort_ro` | compared | `178_sort` |
| `sortAsc` | compared | `178_sort`, `193_main_forms` |
| `sortAscAlpha` | compared | `178_sort`, `193_main_forms` |
| `sortDesc` | compared | `178_sort`, `193_main_forms` |
| `sortDescAlpha` | compared | `178_sort`, `193_main_forms` |
| `srem` | compared | `098_srem` |
| `sscan` | compared | `173_scans`, `193_main_forms` |
| `ssubscribe` | compared | `tests/pubsub.rs` |
| `strlen` | compared | `005_strlen`, `193_main_forms` |
| `subscribe` | compared | `tests/pubsub.rs` |
| `sunsubscribe` | compared | `tests/pubsub.rs` |
| `swapdb` | compared | `193_main_forms` |
| `time` | compared | `174_server` |
| `ttl` | compared | `041_ttl` |
| `type` | compared | `032_type` |
| `unlink` | compared | `028_unlink` |
| `unsubscribe` | compared | `tests/pubsub.rs` |
| `unwatch` | compared | `171_watch` |
| `watch` | compared | `171_watch` |
| `wait` | compared | `174_server` |
| `xack` | compared | `172_stream_groups` |
| `xadd` | compared | `147_xadd`, `148_xrange`, `149_xrevrange` and 11 more |
| `xautoclaim` | compared | `172_stream_groups` |
| `xclaim` | compared | `172_stream_groups` |
| `xdel` | compared | `151_xdel` |
| `xgroup` | compared | `154_xgroup`, `172_stream_groups` |
| `xinfo` | compared | `172_stream_groups` |
| `xlen` | compared | `150_xlen` |
| `xpending` | compared | `172_stream_groups` |
| `xrange` | compared | `148_xrange`, `169_pipeline`, `170_multi` and 2 more |
| `xread` | compared | `153_xread`, `181_prefix` |
| `xreadgroup` | compared | `172_stream_groups` |
| `xrevrange` | compared | `149_xrevrange` |
| `vadd` | not implemented | Throws "Redis::vadd is not implemented yet". |
| `vsim` | not implemented | Throws "Redis::vsim is not implemented yet". |
| `vcard` | not implemented | Throws "Redis::vcard is not implemented yet". |
| `vdim` | not implemented | Throws "Redis::vdim is not implemented yet". |
| `vinfo` | not implemented | Throws "Redis::vinfo is not implemented yet". |
| `vismember` | not implemented | Throws "Redis::vismember is not implemented yet". |
| `vemb` | not implemented | Throws "Redis::vemb is not implemented yet". |
| `vrandmember` | not implemented | Throws "Redis::vrandmember is not implemented yet". |
| `vrange` | not implemented | Throws "Redis::vrange is not implemented yet". |
| `vrem` | not implemented | Throws "Redis::vrem is not implemented yet". |
| `vsetattr` | not implemented | Throws "Redis::vsetattr is not implemented yet". |
| `vgetattr` | not implemented | Throws "Redis::vgetattr is not implemented yet". |
| `vlinks` | not implemented | Throws "Redis::vlinks is not implemented yet". |
| `xtrim` | compared | `152_xtrim` |
| `zAdd` | compared | `110_zadd`, `111_zcard`, `112_zcount` and 39 more |
| `zCard` | compared | `111_zcard` |
| `zCount` | compared | `112_zcount` |
| `zIncrBy` | compared | `117_zincrby` |
| `zLexCount` | compared | `124_zlexcount` |
| `zMscore` | compared | `116_zmscore` |
| `zPopMax` | compared | `130_zpopmax` |
| `zPopMin` | compared | `129_zpopmin`, `189_json_collections` |
| `zRange` | compared | `118_zrange`, `143_zrangestore`, `169_pipeline` and 5 more |
| `zRangeByLex` | compared | `122_zrangebylex` |
| `zRangeByScore` | compared | `120_zrangebyscore` |
| `zrangestore` | compared | `143_zrangestore` |
| `zRandMember` | compared | `131_zrandmember` |
| `zRank` | compared | `114_zrank` |
| `zRem` | compared | `125_zrem` |
| `zRemRangeByLex` | compared | `128_zremrangebylex` |
| `zRemRangeByRank` | compared | `126_zremrangebyrank` |
| `zRemRangeByScore` | compared | `127_zremrangebyscore` |
| `zRevRange` | compared | `119_zrevrange` |
| `zRevRangeByLex` | compared | `123_zrevrangebylex` |
| `zRevRangeByScore` | compared | `121_zrevrangebyscore` |
| `zRevRank` | compared | `115_zrevrank` |
| `zScore` | compared | `113_zscore`, `189_json_collections` |
| `zdiff` | compared | `138_zdiff` |
| `zdiffstore` | compared | `141_zdiffstore` |
| `zinter` | compared | `136_zinter` |
| `zintercard` | compared | `142_zintercard` |
| `zinterstore` | compared | `139_zinterstore` |
| `zscan` | compared | `173_scans`, `193_main_forms` |
| `zunion` | compared | `137_zunion` |
| `zunionstore` | compared | `140_zunionstore` |
<!-- /inventory -->
