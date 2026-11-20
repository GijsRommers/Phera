# Refine repeated named static calls in guards

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Reuse the fact from a guard on a named static call when its class,
method and simple arguments match. A relevant local write or object
member write invalidates the fact.

## Why

A check such as `Loader::find($id) !== null` gives PHPStan evidence for
another call with the same argument on that path.
