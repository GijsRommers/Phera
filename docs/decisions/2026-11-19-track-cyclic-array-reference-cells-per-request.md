# Track cyclic array reference cells per request

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Use one request-owned weak index for array reference cells in PHP and native operations. Clear the remaining cells when the request state drops.

## Why

A pure array cycle can outlive every VM root and has no object for the object collector to find. A weak handle preserves its PHP lifetime through shutdown.

## Turned down

Clearing cells while PHP can still run or adding a second object-destructor pass.
