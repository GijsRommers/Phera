# Gzip files use the PHP stream resource and run file I/O and explicit close on Tokio's blocking pool

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

Gzip files use the PHP stream resource and run file I/O and explicit close on Tokio's blocking pool.

## Why

Ordinary stream functions and `gz*` aliases share the same handle and cursor.

## Turned down

A gzip-only resource that cannot be passed to other stream functions.
