# Bound stream filter chains and output

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

A filter chain accepts at most sixteen stages. A filter may produce at most
8 MiB of intermediate output per bucket. A stream can process any number
of bounded buckets. The URI parser counts nested filter wrappers
toward the depth limit and checks the limit before opening the resource.
Filter output that exceeds the limit aborts the transformation.

## Why

Some filters can expand a small input substantially. Chaining several
expanding filters on an HTTP worker must have a predictable bound,
including when the URI comes from a request.

## Turned down

Allocating an unbounded intermediate buffer and checking its size after the
entire chain has run.
