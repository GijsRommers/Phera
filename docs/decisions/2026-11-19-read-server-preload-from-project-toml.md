# Read the server preload setting from project TOML

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Read `[server].preload` from the project TOML using the TOML parser.

## Why

The server can share the project file with the checker and linter, including standard TOML comments and quoted keys.

## Turned down

Adding another line-based parser for one server option.
