# PDO database and oracle tests

Start the two dedicated databases (leave the owner's port 5432 alone):

```sh
docker run -d --name phera-test-postgres -e POSTGRES_DB=phera_test -e POSTGRES_USER=phera -e POSTGRES_PASSWORD=phera -p 54329:5432 postgres:17
docker run -d --name phera-test-mysql -e MYSQL_DATABASE=phera_test -e MYSQL_USER=phera -e MYSQL_PASSWORD=phera -e MYSQL_ROOT_PASSWORD=phera-root -p 33069:3306 mysql:8.4
```

Wait until both accept connections. PHP 8.5 at `/opt/homebrew/bin/php` must have
`pdo_pgsql` and `pdo_mysql`. Each oracle script creates a uniquely named table and
drops it in `finally`; run the suite serially rather than launching duplicate suites.
There are 94 original scripts, plus fetch/binding/factory/transaction/metadata/type oracles.
The original scripts include five application scenarios per driver.

```sh
cargo test -p phera-ext-pdo -- --nocapture
PHERA_BIN=/absolute/path/to/phera cargo test -p phera-ext-pdo --test oracle -- --nocapture
PHERA_BIN=/absolute/path/to/phera cargo test -p phera-ext-pdo --test concurrency -- --nocapture
cargo fmt -p phera-ext-pdo --check
cargo clippy -p phera-ext-pdo --no-deps --all-targets -- -D warnings
```

Without `PHERA_BIN`, the CLI tests print a skip notice. Live pool tests require both databases and fail if unavailable. Some older tests
print a skip notice without the databases or PHP. Once prerequisites exist, mismatches fail;
there are no expected-failure exclusions. Outputs are saved in the task's authorized
scratch directory under `pdo/oracle`. `native.rs` calls registered methods directly;
it does not prove compiler/VM integration. `concurrency.rs` runs the actual CLI with
one worker, requires successful responses, and checks forty database waits finish
within two seconds for each driver.

## Current integration blockers

The committed runtime registers PDO. Native class constant linkage depends on the VM;
use the numeric scratch-copy option below when it is not available in the chosen HEAD.

The extension API cannot throw an exception with a string code and properties.
PDO exception SQLSTATE codes and `errorInfo` need a native throw payload supporting
`code: Value` and `properties: PhpArray`. Exception-parity scripts retain the real
PHP expectations. See the scratch `REPORT-pool.md` for current results and remaining limitations.

## Pool regression tests

The pool is process-wide and owns a dedicated I/O runtime. Tests create independent
current-thread runtimes on several OS threads; no worker-local pool is involved.
`cargo test -p phera-ext-pdo` includes the live pool contract against both containers.
The pool tests use a separate `phera_pool` account so server connection counts are
not contaminated by other tests using `phera`. Create it in these test containers:

```sh
docker exec phera-test-postgres psql -U phera -d phera_test \
  -c "CREATE ROLE phera_pool LOGIN PASSWORD 'phera_pool'"
docker exec phera-test-mysql mysql -uroot -pphera-root \
  -e "CREATE USER 'phera_pool'@'%' IDENTIFIED BY 'phera_pool'; \
      GRANT ALL ON phera_test.* TO 'phera_pool'@'%';"
```

`make ci-prerequisites` creates that account too, which is how CI gets it.

Application settings live in the environment and are documented in [../README.md](../README.md).
Only `PHERA_ATTR_POOL` (100000, default true) is a constructor option. The previous
per-connection size/timeout attributes no longer exist. `ATTR_PERSISTENT` changes
no pooling behavior. Statements retain their parent's lease; reset is asynchronous.

Pool tests cover N=3 with 13 workers and server counts, cross-worker reuse, FIFO admission,
timeouts, cancellation while waiting and during a query, idle/lifetime retirement, dead
socket replacement, failed reset, and session/credential isolation. The unit test also
prints paired query-dispatch and reset timings; use a release build without competing
benchmarks for useful measurements.


## SQLite (no database services needed)

The `sqlite_*.php` scripts run in `sqlite_oracle.rs`; the existing server-driver
scripts remain in `oracle.rs`. Each SQLite process receives a private temporary
directory in `$argv[1]` and as its working directory. Neither suite substitutes
expected output: it compares the real PHP oracle's stdout with the release CLI.

```sh
cargo build --release -p phera-cli
PHERA_BIN="$PWD/target/release/phera" cargo test -p phera-ext-pdo \
  --test sqlite_oracle --test sqlite_file_format --test sqlite_concurrency -- --nocapture
PHERA_BIN="$PWD/target/release/phera" cargo test -p phera-ext-sqlite3 --test oracle -- --nocapture
```

The file-format test exchanges a database between PHP PDO and Phera SQLite3 in both
directions. The HTTP test owns its temporary files, servers and ephemeral ports;
it compares 200 concurrent WAL requests on two workers, then checks an overlapping
slow and quick request on one Phera worker. It never contacts PostgreSQL or MySQL.

See [SQLite implementation status](../../../sqlite/README.md) for feature coverage,
compile options, lifecycle, callback/stream blockers and remaining differences.
