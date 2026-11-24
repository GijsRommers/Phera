# Close PHP stream filters at last release

- Date: 2026-11-21
- Area: Stream filters
- Decided by: Design

## Decision

Attaching a PHP filter keeps the underlying stream available to the
request's existing resource release callback. At the last PHP reference,
the VM drains closing filter buckets, calls `onClose()`, and closes
the backing. The filter's `$stream` property is null during that final
call. An explicit close can still give the filter a live stream.

## Why

A filter can emit bytes when the last reference goes away. PHP callbacks
need a VM safe point, and those bytes still need the backing stream.
Installing the release guard on attachment keeps ordinary streams on
their existing release path.
