# SQLite implementation status

This branch adds the shared `phera-sqlite` engine, PDO's `sqlite:` driver and the
native `phera-ext-sqlite3` extension. It is **partial**, not a complete replacement
for PHP's SQLite extensions. The PR remains draft because the omissions below
are part of the requested scope.

## Engine, threads and ownership

`rusqlite 0.43.0` uses `libsqlite3-sys 0.40.0` with `bundled`. SQLite is compiled
from its amalgamation, never linked from the system. A C dependency is necessary
for SQLite's file format, locking and SQL semantics; there is no byte-compatible
pure Rust replacement. No project-owned unsafe code or FFI is introduced.
Tokio supplies channels and task scheduling; `tempfile` supplies owned backup
snapshot files. Both PHP extensions depend on this engine, not on each other's
PHP classes.

Each open database owns one OS thread, a current-thread Tokio runtime, and a local
task set. The connection and every borrowing native statement live on that thread.
Opening, preparing, stepping, resetting, finalizing, closing, busy waits, backup
and restore all execute there. Thread creation and snapshot unlinking use Tokio's
blocking pool. Workers exchange owned `Send` commands, values and diagnostics.
There is no `Connection` shared concurrently across threads.

PDO dispatches connection operations through its existing database I/O runtime;
SQLite statements await their owning thread directly. SQLite3 uses the same
channel engine. Cursors remain live between fetches, retaining SQLite's locks.
Dropping the last statement handle queues finalization before subsequent commands
on the connection. Explicit database close waits for statement finalization and
connection closure. Statements retain their parent database until dropped.

**No SQLite handles are pooled**, including file handles and connections requested
with `ATTR_PERSISTENT`. Attached databases, temporary schema, pragmas and future
user functions cannot be reset completely with the available safe API. Every
`:memory:` object gets a distinct connection and database. Concurrent requests
therefore consume one database thread per open SQLite connection.

## Version and compile options

On 2026-11-17 on the development machine, both PHP 8.5.12 (`php -n`) and Phera report SQLite
**3.54.1**. The complete observed options and selected pragma defaults are in
[php-compile-options.txt](php-compile-options.txt) and
[phera-compile-options.txt](phera-compile-options.txt). These are dated development
observations, not expectations pinned into cross-platform tests. CI's system
SQLite version can differ from Phera's bundled version.

`.cargo/config.toml` adjusts bundled compilation to match PHP's observed features:
FTS3/5, JSON (built into modern SQLite), R*Tree, Geopoly, math and percentile
functions, column metadata, STAT4, session/preupdate hooks, memory management and
unlock notification. Foreign keys default off; recursive triggers, auto-vacuum,
page/cache/temp-store defaults and the 250000-variable limit match the oracle.

Observed option differences:

| Option | PHP | Phera | Consequence |
| --- | --- | --- | --- |
| `COMPILER` | clang 21 | clang 17 | Build-tool identity; no observed SQL difference. |
| `ENABLE_LOAD_EXTENSION` | Not listed | Listed | Compiled capability only; PHP-facing loading is refused and SQL loading remains disabled. |
| `HAVE_ISNAN` | Not listed | Listed | Platform detection; numeric comparisons show no resulting difference. |

The local PHP PDO build can enable dynamic extension loading. Phera deliberately
refuses it, using `Unable to load extension "<name>"`. SQLite3 reports
`SQLite Extensions are disabled`, matching the oracle's SQLite3 configuration.

## PHP surface

“Done” below describes implemented, compared behavior, not proof of every hostile
combination. Remaining concrete differences are listed separately.

| Class or surface | Status | Implemented behavior / limitation |
| --- | --- | --- |
| PDO SQLite driver / `Pdo\Sqlite` | PARTIAL | DSNs, native bindings, numeric/text/blob/null values, stringify, quoting, inserts, row counts, multi-statement exec, transactions, timeouts, metadata, supported attributes, subclass/factory and constants. Callbacks, blob streams and explain-mode mutation are absent. |
| `SQLite3` | PARTIAL | `__construct`, `open`, `close`, `exec`, `query`, `querySingle`, `prepare`, `escapeString`, `lastInsertRowID`, error getters, `changes`, `busyTimeout`, exception/extended-code switches, `version`, loading refusal, basic `backup`. See backup, callbacks and query-result-use gaps below. |
| `SQLite3Stmt` | PARTIAL | `bindValue`, `bindParam`, `execute`, `reset`, `clear`, `close`, `paramCount`, `readOnly`, `getSQL`, `busy`, `explain`, private construction and constants. `setExplain`, some diagnostic edges and SQL text truncation remain. |
| `SQLite3Result` | PARTIAL | `fetchArray`, `fetchAll`, `numColumns`, `columnName`, `columnType`, `reset`, `finalize`, private construction; NUM/ASSOC/BOTH and reset/re-execution behavior. Diagnostic caveat below applies. |
| `SQLite3Exception` | DONE | Native exception class, SQLite codes and warning/exception dispatch. |
| Constants | DONE | `SQLITE3_*`, SQLite3 authorizer constants, PDO legacy constants/deprecations, `Pdo\Sqlite` constants and statement explain constants. |

PDO SQLite does not support `ATTR_EMULATE_PREPARES` in the PHP oracle: setting it
returns false, getting it reports IM001, and constructor options do not enable it.
Phera follows that behavior and always uses native SQLite statements. `?`, `?NNN`,
`:name`, `@name` and `$name` are parsed by SQLite itself. PHP PDO's binding-name
normalization still prepends `:`; numeric binding accesses the other native forms.
SQLite3 also accepts an explicit `@name` key, like PHP.

Blob bytes, NUL and invalid UTF-8 are preserved. PDO text preserves NUL. The local
PHP SQLite3 oracle truncates fetched TEXT at NUL, while BLOB does not; Phera follows
that observed distinction. Integers use signed 64-bit values; reals use f64.

## Not finished

* **User SQL callbacks and authorizers:** `createFunction`, `createAggregate`,
  `createCollation`, `setAuthorizer` and legacy `sqliteCreate*` methods are absent.
  PHP callbacks contain worker-local Rc values; rusqlite calls its callbacks
  synchronously on the database thread. A correct implementation needs the worker
  to service callback requests while its query awaits, including reentrant calls
  to the same database. Simply blocking the actor waiting for a worker callback
  deadlocks when that callback uses the actor. This change supplies no such bridge
  and never executes PHP callbacks on a database thread. Legacy method deprecation
  emission is therefore also absent; legacy constant deprecations are implemented.
* **Blob streams:** neither `Pdo\Sqlite::openBlob` nor `SQLite3::openBlob` is exposed.
  The runtime has no PHP stream resource interface (`fopen`/`fread` integration)
  to return. Byte-string BLOB parameters and results do work.
* **Backup:** a consistent source page snapshot is restored into the destination,
  including memory databases. rusqlite requires exclusive mutable access to the
  destination connection. An outstanding destination statement, even an idle one,
  causes `destination has outstanding prepared statements`; PHP can back up with
  idle destination statements. Attached-schema/error/locking parity is incomplete.
* **Explain mutation:** PDO `ATTR_EXPLAIN_STATEMENT` and SQLite3Stmt `setExplain`
  setters are absent. rusqlite exposes the getter but not a safe setter. No custom
  FFI was added to bypass that limitation.
* **Unused query return:** PHP's SQLite3 `query`/`querySingle` can execute a whole
  batch when the return value is unused. The native Context does not expose that
  VM return-use information; Phera always follows the prepared first-statement
  path. `exec` executes complete batches.
* **Diagnostic/state edges:** last-error state is tracked by the wrapper, since
  rusqlite has no safe getter for connection `sqlite3_errcode`/`errmsg`. Ordinary
  failures, extended constraints, warnings and row/done codes are compared, but
  all stale-error combinations are not equivalent. PDO invalid-column metadata
  after EOF can differ depending on prior metadata access. Invalid SQLite3 binding
  type numbers also differ from this PHP build's fatal formatter diagnostic.
* **SQL representation:** `getSQL(false)` retains the submitted SQL rather than
  SQLite's first-statement-only text. SQL strings and filenames cross rusqlite's
  UTF-8 API as Rust strings; invalid UTF-8 in those inputs is lossy. Data values
  use byte-preserving binding APIs.
* Other pre-existing PDO limitations such as `bindColumn`, iteration,
  `debugDumpParams`, special fetch modes and driver statement options are not
  completed by this SQLite change.

These are implementation gaps, not excluded passing comparison cases. Failed
comparisons found during development were retained and fixed. No original PDO
script was removed or its expectation changed.

## Validation

The release CLI is selected with `PHERA_BIN`; PHP is resolved by the shared
`find_php()` oracle helper and run with `-n`. See
[PDO test commands](../extensions/pdo/tests/README.md#sqlite-no-database-services-needed).

* 73 PDO SQLite and 47 SQLite3 scripts: **120 matches, zero mismatches**.
* Numeric matrices compare 3030 PDO and 1515 SQLite3 executions. Reference
  matrices add 180 execution/fetch scenarios: **4725 generated scenarios**, or
  **9450 execute/fetch calls**, in addition to the other scripts' calls.
* PHP-created files read through Phera and Phera-created files read through PHP:
  identical header, integer/float/blob values and successful integrity checks.
* HTTP WAL test: 200 concurrent requests, two workers, **zero failures on both
  PHP and Phera**. A separate one-worker run finishes a quick query while a slow
  query is still running. Wall-clock numbers are evidence of overlap, not a
  performance comparison.
* Engine regressions exercise 100 cursor-release cycles, independent memory
  databases, statement retention and explicit close invalidation.
* 313 workspace tests excluding the service-dependent PDO package pass locally.
  After the constructor diagnostic correction, all 48 VM tests pass again.
  Formatting, workspace Clippy and the no-unsafe guard pass locally.
  Existing PostgreSQL/MySQL integration and pool tests require CI's owned services;
  no port 5432 was contacted. A combined Cargo command accidentally entered the
  existing PDO availability probes on ports 54329/33069 and was interrupted before
  any server-driver script ran. This does not establish server-driver parity.

CI's PHP extension lists, wrapper and prerequisite checker now require both
`pdo_sqlite` and `sqlite3`; the release comparison job includes the new extension.
Linux CI and review status must be checked on the PR; local results do not establish
either. The full feature-completeness gate is not met because of the gaps above.
