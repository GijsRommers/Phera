# SQLite3 in Phera

PHP's `SQLite3`, `SQLite3Stmt`, `SQLite3Result` and `SQLite3Exception`, with
the `SQLITE3_*` constants:

```php
$db = new SQLite3(':memory:');
$db->exec('CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)');
$statement = $db->prepare('INSERT INTO t (name) VALUES (:name)');
$statement->bindValue(':name', 'Ada', SQLITE3_TEXT);
$statement->execute();
var_dump($db->querySingle('SELECT name FROM t', true));
```

The engine, its thread per database, and what is not implemented (SQL
callbacks, blob streams, explain mode, batch `query()`) are described in
`crates/sqlite/README.md`; the same engine serves PDO's `sqlite:` driver.

## Tests

```sh
cargo build --release -p phera-cli
PHERA_BIN="$PWD/target/release/phera" cargo test -p phera-ext-sqlite3 -- --nocapture
```

Each script in `tests/scripts` runs through PHP 8.5 (`sqlite3` loaded) and
through Phera, each in a directory of its own for its database files, and must
print the same stdout and stderr. No database server is needed.
