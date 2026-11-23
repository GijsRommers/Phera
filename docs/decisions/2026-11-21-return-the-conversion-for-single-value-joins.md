# Return the conversion for single-value joins

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Convert and return the sole value directly when a synchronous join has
one entry. With multiple entries, the byte buffer still joins them in
iteration order.

## Why

Doctrine uses a join to build identity keys from mapped identifiers.
A one-entry array has no separator to add. Returning the converted value
also lets an existing PHP string share its bytes under copy-on-write.

## Turned down

An array of stringified values would allocate additional temporary
storage and repeat the conversion work.
