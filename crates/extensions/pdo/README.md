# PDO in Phera

PHP's PDO with its PostgreSQL (`pgsql:`), MySQL (`mysql:`) and SQLite
(`sqlite:`) drivers. PostgreSQL uses Phera's `postgres-protocol` client,
MySQL uses `mysql_async` and SQLite uses `crates/sqlite`.

```php
$pdo = new PDO('pgsql:host=127.0.0.1;dbname=app', $user, $password);
$statement = $pdo->prepare('SELECT id, name FROM users WHERE id = :id');
$statement->execute(['id' => 42]);
$user = $statement->fetch(PDO::FETCH_ASSOC);
```

Every database wait is asynchronous: while one request waits for its query,
its worker thread runs other requests. The drivers sit behind one interface,
`src/drivers/mod.rs`, which also says what adding a driver takes.

SQLite differs in two ways, described in `src/drivers/sqlite/mod.rs`: its
calls block, so each database runs on a thread of its own and a statement
streams its rows from a live cursor there; and its handles are never pooled.
`Pdo\Sqlite` and the SQLite attributes are there; what SQLite does not have
yet (SQL callbacks, blob streams) is listed in `crates/sqlite/README.md`.

## Connection pool

PostgreSQL and MySQL run on the event loop of the PHP worker that opens the
socket. Workers share an admission limit per database. On a worker,
`new PDO` calls can reuse an idle session with matching DSN settings,
connection options and credentials, including TLS settings.

A PDO object borrows one connection for itself; its statements keep it
borrowed after the PDO variable is gone. When the last of them is freed, the
connection is reset in the background and lent again: PostgreSQL rolls back
an open transaction and runs `DISCARD ALL`; MySQL gets its reset-connection command
(change-user on older servers), then restores the database, charset, initial
SQL and autocommit settings. A
connection whose reset fails is closed. A connection interrupted halfway
through a query is closed and the server rolls back its transaction.

When every connection is busy, `new PDO` waits in arrival order, and after
the wait time throws a PDOException that names the pool limit. A worker
with an idle socket can close it to let another worker connect. Idle and old
connections are closed; one idle for more than three seconds is checked with
a round trip before it is lent.

| Environment setting | Default | |
| --- | ---: | --- |
| `PHERA_PDO_POOL_SIZE` | 20 | open pooled connections per database, over all workers |
| `PHERA_PDO_POOL_IDLE_SECONDS` | 60 | how long a returned connection may stay idle |
| `PHERA_PDO_POOL_LIFETIME_SECONDS` | 1800 | how old a connection may get; a borrowed one retires when returned |
| `PHERA_PDO_POOL_WAIT_SECONDS` | 30 | how long `new PDO` waits for a connection |
| `PHERA_PDO_IO_THREADS` | | accepted for compatibility and has no effect |

They are read once, on first use; a script cannot change them.
PDO reports `PDO::ATTR_PERSISTENT` separately from Phera's pool policy. A connection can opt
out; it then counts against no limit and is closed when freed:

```php
$pdo = new PDO($dsn, $user, $password, [PDO::PHERA_ATTR_POOL => false]);
```

## TLS

Both server drivers negotiate TLS 1.2 or 1.3 on the worker's asynchronous event loop
using rustls and ring's default cipher suites.
Phera loads the platform's trust roots and uses the bundled WebPKI roots when
the platform store is unavailable or empty. A supplied CA takes precedence.

| PostgreSQL `sslmode` | Connection behavior |
| --- | --- |
| `disable` | Plain TCP |
| `allow` | Plain TCP first, followed by TLS if the server rejects plaintext |
| `prefer` | TLS first, followed by plain TCP when the server declines TLS |
| `require` | TLS, with chain verification when `sslrootcert` is supplied |
| `verify-ca` | TLS with chain verification |
| `verify-full` | TLS with chain and hostname verification |

Use `sslrootcert=/path/ca.pem` for a private PostgreSQL CA. The `sslcert` and
`sslkey` parameters select a PEM client certificate and private key.

The PostgreSQL DSN option `channel_binding=prefer` uses SCRAM channel binding
when the server offers it over TLS. Use `disable` to opt out or `require` to
require it. With `require_auth`, list permitted methods such as
`scram-sha-256,md5` or excluded methods such as `!password,!md5`. The
`none` method accepts a server that skips the authentication challenge.
`require_auth` is optional and defaults to the server's choice.

For MySQL, use `Pdo\Mysql::ATTR_SSL_CA` or `ATTR_SSL_CAPATH` for trust roots,
and `ATTR_SSL_CERT` with `ATTR_SSL_KEY` for client authentication. Server
verification is enabled by default. With TLS options present, setting
`ATTR_SSL_VERIFY_SERVER_CERT` to `false` explicitly disables verification.
`ATTR_SSL_CIPHER` accepts PHP's cipher-list string and requests TLS. Rustls
negotiates from ring's default TLS 1.2 and 1.3 suites. The supplied list has
no effect on negotiation.

`PDO::ATTR_CLIENT_VERSION` identifies Phera's client as `mysql_async 0.37.1`
or `phera-postgres 3.0`. The PostgreSQL client accepts messages up to 64 MiB,
including the four-byte length field, and closes the connection on invalid
framing.

PDO PostgreSQL rejects embedded NUL bytes in SQL or text parameters with an
error and the connection remains usable. PHP 8.5 truncates these inputs at
the first NUL.

## What is not there

These throw a PDOException that says "... is not supported yet":

- Fetching into a class other than stdClass (`FETCH_CLASS`, `FETCH_INTO`,
  `fetchObject('Foo')`), `FETCH_LAZY`, `FETCH_BOUND`, `FETCH_FUNC`,
  scrollable cursors, `bindColumn`, `nextRowset`,
  `getIterator`, `debugDumpParams` and the statement attributes.
- LOB parameters and streams: `PARAM_LOB`, PostgreSQL `bytea`, output
  parameters, the `pgsqlCopy*` methods, statement driver options.
- Quoting a string that is not UTF-8, or one with a NUL byte for
  PostgreSQL; non-UTF-8 values with emulated prepares.
- PostgreSQL types other than the numeric, text, JSON, date and time,
  network, UUID, enum and array types.
- MySQL's pinned authentication public key option. The driver can request
  the server's RSA public key for `caching_sha2_password` on a plain connection.

MySQL `LOCAL INFILE` and its directory option are refused because clients would
otherwise allow the server to request files from the application host.

## Tests

The tests use two databases of their own; never the server on port 5432:

```sh
docker run -d --name phera-test-postgres -e POSTGRES_DB=phera_test -e POSTGRES_USER=phera \
    -e POSTGRES_PASSWORD=phera -p 54329:5432 postgres:17
docker run -d --name phera-test-mysql -e MYSQL_DATABASE=phera_test -e MYSQL_USER=phera \
    -e MYSQL_PASSWORD=phera -e MYSQL_ROOT_PASSWORD=phera-root -p 33069:3306 mysql:8.4
make ci-prerequisites      # checks PHP and both databases, creates the phera_pool account
cargo build --release -p phera-cli
PHERA_BIN="$PWD/target/release/phera" cargo test -p phera-ext-pdo -- --nocapture
```

PHP 8.5 with `pdo_pgsql` and `pdo_mysql` is the oracle (see CONTRIBUTING.md
for how it is found).

- `tests/scripts`: each `pgsql_` and `mysql_` script runs through PHP and through Phera, which
  must succeed with the same stdout and stderr. Each creates a table of its
  own and drops it in `finally`. Without `PHERA_BIN` they are skipped.
- `tests/native_oracle.rs`: the registered methods called directly, against
  the same calls in PHP: error state, bindings and quoting, and every value
  type crossed with emulation, `ATTR_STRINGIFY_FETCHES` and five fetch modes.
- `tests/native*.rs`: binding by reference, rollback when a PDO is dropped,
  forty overlapping waits on one thread, statements that keep their lease,
  a cancelled request.
- `src/drivers/postgres/tls_tests.rs` uses a local TLS proxy in front of the
  plain PostgreSQL test service. `src/drivers/mysql/auth_tests.rs` uses a
  local server to force the full TLS and RSA authentication exchanges.
- `tests/sqlite_*.rs`: the `sqlite_` scripts, each run in a directory of its
  own; a database written by PHP and read by Phera and the other way round;
  200 simultaneous WAL writes through `php -S` and `phera serve`, and a
  quick request answered while a slow query runs on the same worker. These
  need no database server.
- `tests/concurrency.rs`: forty 0.3 s queries through `phera serve
  --workers 1` must finish within two seconds.
- `src/pool/tests`: the pool against the live servers, as the `phera_pool`
  account, which no other test uses, so that the connection counts the
  server reports are the pool's own: reuse and reset, limits with 13
  workers, FIFO order, timeouts, cancellation, expiry, killed connections,
  failed resets, and separate pools for separate settings. Another process
  logged in as `phera_pool` at the same time makes the count check fail.
