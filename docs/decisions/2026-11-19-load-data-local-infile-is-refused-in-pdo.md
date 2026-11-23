# `LOAD DATA LOCAL INFILE` is refused in PDO and `mysqli`

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

`LOAD DATA LOCAL INFILE` is refused in PDO and `mysqli`.

## Why

It lets a database server read files of the client.

## Turned down

Supporting it behind an option.
