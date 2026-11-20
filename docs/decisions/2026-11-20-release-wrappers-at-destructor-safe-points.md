# Release wrappers at destructor safe points

- Date: 2026-11-20
- Area: Runtime
- Decided by: Design

## Decision

A PHP user-wrapper handle carries a guard that queues its close callback
on the object's existing destructor queue when the last resource owner
releases it. Explicit close cancels the guard. A worker uses the existing
request teardown for handles still held at shutdown.

## Why

Wrapper callbacks can suspend, throw or drop other values. Running them
at the interpreter's existing release boundaries provides PHP's immediate
close timing and leaves ordinary native streams on the same path.

## Turned down

Polling the wrapper registry after every instruction.
