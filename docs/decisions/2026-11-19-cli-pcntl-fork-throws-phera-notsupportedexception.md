# CLI `pcntl_fork()` throws `Phera\NotSupportedException`

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

CLI `pcntl_fork()` throws `Phera\NotSupportedException`.

## Why

Forking leaves one thread in the child, which inherits runtime and allocator locks that other threads may have held.

## Turned down

Calling Unix `fork()` and continuing to run the interpreter in the child.
