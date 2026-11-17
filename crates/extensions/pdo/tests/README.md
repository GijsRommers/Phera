# PDO database and oracle tests

Start the two dedicated databases (leave the owner's port 5432 alone):

```sh
docker run -d --name phera-test-postgres -e POSTGRES_DB=phera_test -e POSTGRES_USER=phera -e POSTGRES_PASSWORD=phera -p 54329:5432 postgres:17
docker run -d --name phera-test-mysql -e MYSQL_DATABASE=phera_test -e MYSQL_USER=phera -e MYSQL_PASSWORD=phera -e MYSQL_ROOT_PASSWORD=phera-root -p 33069:3306 mysql:8.4
```

Wait until both accept connections. PHP 8.5 (set `PHERA_PHP=/absolute/path/to/php`) must have
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
there are no expected-failure exclusions. Outputs are saved under the system
temporary directory at `phera-pdo-tests/oracle`. `native.rs` calls registered methods directly;
it does not prove compiler/VM integration. `concurrency.rs` runs the actual CLI with
one worker, requires successful responses, and checks forty database waits finish
within two seconds for each driver.

CI builds the release CLI, exports `PHERA_BIN`, and verifies the PHP version,
extensions and both database connections before running `cargo test --workspace`.
`make ci` performs the same checks locally. Missing prerequisites fail this preflight
instead of silently skipping the database suites.
