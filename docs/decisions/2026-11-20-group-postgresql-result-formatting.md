# Group PostgreSQL result formatting

- Date: 2026-11-20
- Area: PostgreSQL driver
- Decided by: Design

## Decision

Send one formatting statement per group of up to 64 rows. Bind the binary
values with their original PostgreSQL types, preserve nulls, and order the
reply by the original row position. Apply this to PDO's server-formatted
columns and to pgsql's extended query results. PDO queries without columns
that need server formatting continue to use a single query.

## Why

Previously, each row sent its own formatting statement. A result with
timestamps could make an extra round trip for every row. Groups of 64 cover
ordinary application pages in one trip and bound the parameters of a
formatting statement. PostgreSQL still applies the connection's DateStyle,
TimeZone, IntervalStyle and other session settings.

## Follow-up

This is an interim step. A client that requests text results in the original
Bind message can remove the formatting statement and this grouping code.
The client choice and implementation are a separate design task.
