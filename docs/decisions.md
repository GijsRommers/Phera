# Design decisions

## 2026-11-19 — Shared SQL drivers

Move the async SQL clients, TLS configuration, wire result conversion, and lease
registry to `phera-db-drivers`. PDO, mysqli, and pgsql can use the same drivers
without depending on another PHP extension. PDO keeps its PHP argument parsing,
error formatting, and SQLite statement handling. The lease registry preserves
PDO's process-wide pool, option keys, and reset behavior.

Putting SQL-specific code in `phera-io-pool` would add database client and TLS
dependencies to generic I/O pooling. A second client would duplicate protocol
handling, connection cleanup, and TLS behavior.

## 2026-11-19 — Native SQL extension state and result transport

Keep procedural and object calls on the same native handles. Store the mysqli
report flags and the pgsql default connection in private request state. PHP
requests on a worker can interleave, so thread-local report flags or PHP-visible
global variables would leak state or make it script-mutable.

Scope persistent mysqli and pgsql leases to the worker in the shared registry.
Use the existing process-wide key for PDO and unpooled leases for ordinary
mysqli and pgsql connections. This preserves PDO reuse rules and prevents a
persistent session from crossing workers. Retire a mysqli lease after
`change_user()` because the original pool key describes different credentials.

Use the existing protocol clients to get text and binary rows. For pgsql,
observe PostgreSQL command and column headers on the existing socket, and use
the server to format prepared binary result values. This keeps session-sensitive
representations such as dates and arrays without rerunning a statement that can
have side effects. Formatting prepared rows adds a server round trip per row.
Use Tokio tasks to await the same leases for asynchronous PostgreSQL queries.

Refuse MySQL LOCAL INFILE at the API boundary, and keep the driver without a
file handler. Require a server-side single-statement parse when a mysqli query
contains a statement separator. The client negotiates multi-statements and
does not expose a way to toggle that capability after connecting.
