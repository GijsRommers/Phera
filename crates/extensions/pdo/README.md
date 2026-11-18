# PDO in Phera

PHP's PDO with its PostgreSQL (`pgsql:`), MySQL (`mysql:`) and SQLite
(`sqlite:`) drivers, written against `tokio-postgres`, `mysql_async` and
`crates/sqlite`:

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

PostgreSQL and MySQL connections are pooled across the whole process, all `phera serve --workers
N` workers included (`crates/io-pool` shows how a query travels to the I/O
threads and back). A pool is shared by `new PDO` calls with the same driver,
DSN settings and credentials.

A PDO object borrows one connection for itself; its statements keep it
borrowed after the PDO variable is gone. When the last of them is freed, the
connection is reset in the background and lent again: PostgreSQL gets
`ROLLBACK` and `DISCARD ALL`; MySQL gets its reset-connection command
(change-user on older servers) and then its DSN database and charset back. A
connection whose reset fails is closed. A cancelled request takes the same
path, after its query in flight has finished. A PostgreSQL connection that
prepared a custom type is closed rather than reset: tokio-postgres keeps
type statements that `DISCARD ALL` invalidates and its API cannot forget.

When every connection is busy, `new PDO` waits in arrival order, and after
the wait time throws a PDOException that names the pool limit. Idle and old
connections are closed; one idle for more than three seconds is checked with
a round trip before it is lent.

| Environment setting | Default | |
| --- | ---: | --- |
| `PHERA_PDO_POOL_SIZE` | 20 | connections per pool, over all workers |
| `PHERA_PDO_POOL_IDLE_SECONDS` | 60 | how long a returned connection may stay idle |
| `PHERA_PDO_POOL_LIFETIME_SECONDS` | 1800 | how old a connection may get; a borrowed one retires when returned |
| `PHERA_PDO_POOL_WAIT_SECONDS` | 30 | how long `new PDO` waits for a connection |
| `PHERA_PDO_IO_THREADS` | 2 | threads of the database I/O runtime |

They are read once, on first use; a script cannot change them.
`PDO::ATTR_PERSISTENT` is accepted and changes nothing. A connection can opt
out; it then counts against no limit and is closed when freed:

```php
$pdo = new PDO($dsn, $user, $password, [PDO::PHERA_ATTR_POOL => false]);
```

## What is not there

These throw a PDOException that says "... is not supported yet":

- TLS (`sslmode` other than `disable`, the MySQL TLS attributes).
- Fetching into a class other than stdClass (`FETCH_CLASS`, `FETCH_INTO`,
  `fetchObject('Foo')`), `FETCH_LAZY`, `FETCH_BOUND`, `FETCH_FUNC`,
  `FETCH_NAMED`; scrollable cursors; `bindColumn`, `nextRowset`,
  `getIterator`, `debugDumpParams` and the statement attributes.
- LOB parameters and streams: `PARAM_LOB`, PostgreSQL `bytea`, output
  parameters, the `pgsqlCopy*` methods, statement driver options.
- Quoting a string that is not UTF-8, or one with a NUL byte for
  PostgreSQL; non-UTF-8 values with emulated prepares.
- PostgreSQL types other than the numeric, text, JSON, date and time,
  network, UUID, enum and array types.
- The `Pdo\Pgsql` and `Pdo\Mysql` constants and driver methods; most
  driver-specific attributes.

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
