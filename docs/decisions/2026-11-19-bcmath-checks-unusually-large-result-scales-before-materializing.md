# BCMath checks unusually large result scales before materializing powers of ten or formatted output

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

BCMath checks unusually large result scales before materializing powers of ten or formatted output.

## Why

A syntactically valid scale can request gigabytes of storage.

## Turned down

Waiting for the host allocator after starting the computation.
