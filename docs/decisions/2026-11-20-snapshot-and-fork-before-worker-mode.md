# Snapshot and fork is built before worker mode, and worker mode waits for its result

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Owner

## Decision

Snapshot and fork (architecture.md 5.4, steps 1 to 3) is built now, together with work on the interpreter itself. Worker mode (step 5) is not started: it is built only if a booted and forked application still falls short of what applications written for long-running use need.

## Why

A fork gives an application the same saving, no boot work per request, without asking it to be written for it and without giving up the isolation of requests. If that is enough, a second way to run applications is only more to maintain.

## Turned down

Building worker mode first because it is the smaller change. Dropping worker mode from the design: it stays described, so that the choice can be made on measurements.
