# Stream wrapper state belongs to the request

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

A request creates its wrapper registry when PHP first changes it. Each
opened user stream owns a PHP object. Only native stream state travels
to the blocking I/O pool.

## Why

Wrapper callbacks can manipulate resources and suspend in a fiber. Their
state and any changes to built-in schemes belong to the application request
that created them.

## Turned down

A worker-wide registry or sending PHP objects to the blocking pool.
