# phera-sqlite: the SQLite engine behind PDO and SQLite3

```php
$pdo = new PDO('sqlite::memory:');      // PDO's sqlite: driver
$db = new SQLite3('/tmp/app.db');        // the SQLite3 classes
```

Both PHP APIs stand on this crate, not on each other. It uses `rusqlite`
(0.43) with SQLite compiled from its bundled amalgamation, never the system's
library: a C dependency, accepted because nothing else reads and writes
SQLite's file format with SQLite's locking and SQL. The crate itself has no
`unsafe` and no FFI of its own.

## Threads

SQLite has no network protocol to wait on asynchronously: every call does file
I/O and may wait for a lock. So each open database owns an OS thread with a
current-thread Tokio runtime, and the connection and all its statements live
there. Opening, preparing, stepping, resetting, finalizing, closing, busy
waits, backup and restore all run on it. A PHP worker sends owned commands
and awaits the answer, so it serves other requests meanwhile; even creating
the thread and deleting a backup snapshot happen on Tokio's blocking pool.

A statement keeps a live cursor between fetches, as in PHP, which holds
SQLite's read locks until the last row or a reset. Dropping the last handle
of a statement finalizes it before the database's next command; closing the
database waits for that.

**Handles are never pooled**, not even with `PDO::ATTR_PERSISTENT`: attached
databases, temporary tables and pragmas cannot be reset completely, and an
in-memory database is the data itself. Every open SQLite connection therefore
costs a thread for as long as it is open.

## Build options

`.cargo/config.toml` compiles the bundled SQLite with the options PHP 8.5's
build has: FTS3/5, R*Tree, Geopoly, math and percentile functions, column
metadata, STAT4, session and preupdate hooks, foreign keys off by default, and
PHP's page, cache and variable limits. Both reported SQLite 3.54.1 on
2026-11-17; [php-compile-options.txt](php-compile-options.txt) and
[phera-compile-options.txt](phera-compile-options.txt) are the full lists.
They differ in the compiler, in `HAVE_ISNAN` (platform detection) and in
`ENABLE_LOAD_EXTENSION`, which is compiled in but never reachable: PDO refuses
`loadExtension()` and SQLite3 reports "SQLite Extensions are disabled", as
PHP's builds do.

## What is not there

- **PHP callbacks in SQL**: `createFunction`, `createAggregate`,
  `createCollation`, `setAuthorizer` and the old `sqliteCreate*` methods.
  SQLite calls a callback synchronously on the database's thread, while PHP
  values live on the worker; it needs the worker to answer callback requests
  while its query waits, including callbacks that use the same database.
- **Blob streams**: `openBlob` in PDO and SQLite3 (Phera has no PHP stream
  resources yet). BLOB values as strings work.
- **Backup** needs the destination to have no open statement at all; PHP
  allows idle ones.
- **Setting the explain mode**: `Pdo\Sqlite::ATTR_EXPLAIN_STATEMENT` and
  `SQLite3Stmt::setExplain` (rusqlite has no safe setter).
- `SQLite3::query()` and `querySingle()` run only the first statement, also
  when PHP would run a whole batch because the result is unused (the native
  API does not know whether it is). `exec()` runs batches.
- Error state is kept by the wrapper, as rusqlite exposes no
  `sqlite3_errcode`: ordinary failures, extended codes, warnings and row/done
  codes match, but not every stale-error combination. `getSQL(false)` returns
  the SQL as submitted; SQL text and file names that are not UTF-8 are
  converted lossily (bound values keep their bytes).
