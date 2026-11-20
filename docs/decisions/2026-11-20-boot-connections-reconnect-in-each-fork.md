# A database connection the boot made reconnects in each fork

- Date: 2026-11-20
- Area: Server and requests, databases
- Decided by: Claude (follow-up to the owner's "never disabled")

## Decision

A connection opened before `Phera\snapshot()` (WordPress's `$wpdb->dbh` is
a mysqli connection) no longer refuses the snapshot. In each fork its copy
is unconnected; on first use it borrows a session from the shared pool
with the boot's settings (host, database, credentials, options) and
replays, in order, what the boot changed about its session: the statements
of `set_charset()`, `select_db()` and `autocommit()`, and every `SET`,
`USE`, `PRAGMA`, `ATTACH` or `DETACH` statement it ran. The pool resets a
session before lending it again, so no request sees another's session.
Which statements change a session is decided by
`phera_db_drivers::session_log::SessionLog`, shared by mysqli, PDO and
pgsql.

The snapshot is still refused, with the reason, when the session holds what
a replay cannot make again: an open transaction (the driver's own flag, or
`BEGIN`/`START` without `COMMIT`/`ROLLBACK`/`END`), table locks, a
temporary table, an SQL-level `PREPARE`, a cursor (`DECLARE`), `LISTEN`,
statements run with `multi_query()`, a result still streaming, or a user
changed with `change_user()`.

A stored `mysqli_result` is rows in memory; each fork gets its own copy at
the same position.

A boot's non-persistent connection is borrowed from the pool in forks
(`p:` connections stay per worker). `thread_id` and `CONNECTION_ID()` are
the new session's after the first use; `insert_id` and `affected_rows` keep
the boot's values until the request's first statement, as they would in a
run from the top.

## Why

Every refusal sends a script's requests to the from-the-top path, five
times the cost of a fork. A connection is the most common state a boot
holds. Reconnecting per fork costs what a run from the top costs at most;
borrowing from the pool costs nothing after the first request.

## Turned down

- Sharing the boot's session: two requests would talk over one socket.
- Replaying every statement the boot ran: `INSERT` would run again.
- Recognising session changes inside other statements (`SELECT ... INTO
  @x`, `set_config()`, `GET_LOCK()`): first words are a clear, testable
  rule; these are not noted.
