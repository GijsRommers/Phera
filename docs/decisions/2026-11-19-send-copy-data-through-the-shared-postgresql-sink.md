# Send COPY data through the shared PostgreSQL sink and consume traversable row sources without reading their keys

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Send COPY data through the shared PostgreSQL sink and consume traversable row sources without reading their keys.

## Why

Preserves the connection's transaction and authentication and follows PHP's iterator calls.

## Turned down

A separate COPY session or INSERT emulation.
