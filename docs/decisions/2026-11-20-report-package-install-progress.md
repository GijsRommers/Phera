# Report the preparation and installation stages

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Show one changing counter for preparation and one for placement when standard
error is a terminal. Write plain stage lines to redirected output. Keep
verbose per-package materialization messages and the final elapsed-time line.

## Why

Parallel preparation can take time before any vendor files appear.
The stage counters keep the terminal compact while logs retain the stage
boundaries and the final result.

## Turned down

A progress framework dependency for two counters.
