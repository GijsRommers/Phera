# PDO database and oracle tests

Start the two dedicated databases (leave the owner's port 5432 alone):

```sh
docker run -d --name phera-test-postgres -e POSTGRES_DB=phera_test -e POSTGRES_USER=phera -e POSTGRES_PASSWORD=phera -p 54329:5432 postgres:17
docker run -d --name phera-test-mysql -e MYSQL_DATABASE=phera_test -e MYSQL_USER=phera -e MYSQL_PASSWORD=phera -e MYSQL_ROOT_PASSWORD=phera-root -p 33069:3306 mysql:8.4
```

Wait until both accept connections. PHP 8.5 at `/opt/homebrew/bin/php` must have
`pdo_pgsql` and `pdo_mysql`. Each oracle script creates a uniquely named table and
drops it in `finally`; run the suite serially rather than launching duplicate suites.
There are 47 scripts per driver, including five application scenarios per driver.

```sh
cargo test -p phera-ext-pdo -- --nocapture
PHERA_BIN=/absolute/path/to/phera cargo test -p phera-ext-pdo --test oracle -- --nocapture
PHERA_BIN=/absolute/path/to/phera cargo test -p phera-ext-pdo --test concurrency -- --nocapture
cargo fmt -p phera-ext-pdo --check
cargo clippy -p phera-ext-pdo --no-deps --all-targets -- -D warnings
```

Without `PHERA_BIN`, the CLI tests print a skip notice. Without the databases or
PHP, database tests print a skip notice. Once prerequisites exist, mismatches fail;
there are no expected-failure exclusions. Outputs are saved in the task's authorized
scratch directory under `pdo/oracle`. `native.rs` calls registered methods directly;
it does not prove compiler/VM integration. `concurrency.rs` runs the actual CLI with
one worker, requires successful responses, and checks forty database waits finish
within two seconds for each driver.

## Current integration blockers

HEAD 652efcc does not register PDO in the runtime, contrary to the task premise.
The owner needs to add `phera-ext-pdo.workspace = true` to `crates/runtime/Cargo.toml`
and `registry.load(&phera_ext_pdo::Pdo)` to `Runtime::new`. Those files are outside
this implementation's scope. Until then the CLI oracle and concurrency tests fail
with `Class "PDO" not found` rather than being marked as passing or expected failures.

The extension API also cannot throw an exception with a string code and properties.
PDO exception SQLSTATE codes and `errorInfo` need a native throw payload supporting
`code: Value` and `properties: PhpArray`. Exception-parity scripts retain the real
PHP expectations. See the scratch `REPORT.md` for all remaining limitations.
