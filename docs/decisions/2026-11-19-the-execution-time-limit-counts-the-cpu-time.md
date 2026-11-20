# The execution time limit counts the CPU time a request spends running PHP code and is checked where the interpreter already hands over to the scheduler

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

The execution time limit counts the CPU time a request spends running PHP code and is checked where the interpreter already hands over to the scheduler. Process and process-stream waits also count toward the budget. Other native calls check the limit when they return control to the interpreter.

## Why

A check per instruction would slow every script; waiting for a database must not count, as in PHP on Unix.

## Turned down

A wall-clock limit; a clock read in the interpreter loop.
