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
