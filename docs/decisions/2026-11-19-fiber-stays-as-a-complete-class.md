# `Fiber` stays as a complete class

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

`Fiber` stays as a complete class. It is a compatibility API (Monolog and others ask `Fiber::getCurrent()`), not part of Phera's concurrency, which is tasks on Tokio.

## Why

More real code runs unchanged.

## Turned down

Reducing it to `Fiber::getCurrent()` returning null.
