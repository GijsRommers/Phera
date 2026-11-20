# Queue a later error from a mysqli multi-query with its result position

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Queue a later error from a mysqli multi-query with its result position.

## Why

PHP lets callers consume the successful results before the later statement's error. New commands wait until the caller advances through those results.

## Turned down

Reporting a later statement's error as the outcome of the entire call.
