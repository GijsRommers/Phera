# Load the blocking CLI entry on the calling thread

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Read the entry script on the calling thread in the blocking CLI and dump
entry points. Keep the compiler's sized stack and use the same code cache
and PHP execution path as the asynchronous entry point.

## Why

The blocking caller already waits for the entry script. Dispatching its file
lookup to Tokio adds a thread hand-off before the compiler starts. The
asynchronous API and includes still use the blocking pool for file I/O.

## Turned down

Changing the parser stack budget or making file reads synchronous for
asynchronous runtime callers.
