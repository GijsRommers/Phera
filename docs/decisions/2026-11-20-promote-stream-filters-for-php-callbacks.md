# Promote stream filters for PHP callbacks

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Keep native codecs on the native I/O path until a stream needs a PHP
filter or its backing wrapper can call PHP. Move existing codec state
and unread bytes into an ordered request-owned pipeline at that point.

## Why

Prepending, appending and removing a stage must respect its place in the
same pipeline. Native I/O can still run on the blocking executor without
carrying request objects. The request pipeline releases every stage
borrow before a PHP callback can run.

## Turned down

Running all native codecs before or after the PHP stages as a group.
