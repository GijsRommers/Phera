# Use MySQL reply flags for quoting

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Use the `NO_BACKSLASH_ESCAPES` status flag from the client's latest
successful reply for PDO quoting and emulated parameter binding. Ask for
the session mode when the client has no status flags after an error.

## Why

The server reports this bit in its responses. A parameterized text query
previously sent a separate query for the mode before sending its SQL.

## Turned down

Tracking `SET sql_mode` in the SQL text. Server-side code can change the
mode, and the reply is the source that reflects the session.
