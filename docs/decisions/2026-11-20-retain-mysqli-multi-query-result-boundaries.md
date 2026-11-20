# Retain mysqli multi-query result boundaries

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Buffer `mysqli_multi_query()` results until the MySQL client can report the
next-result flag independently of draining the current result.

## Why

`mysqli_more_results()` reads the flag even before PHP fetches the first row.
`mysql_async` 0.37 keeps the flag inside the connection. Its public row stream
crosses the boundary by parsing the next result header, which may be an
error belonging to the next statement.

## Turned down

Guessing the number of result sets from the SQL text or delivering the next
statement's error while the script is still reading the preceding result.
