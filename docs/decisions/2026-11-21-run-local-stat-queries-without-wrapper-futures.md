# Run local stat queries without wrapper futures

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Validate and classify a path once. Complete native metadata queries on
the request worker and return their result in a ready future. A user
stream wrapper still runs its PHP callback through the asynchronous
path. Borrow absolute paths and resolve relative paths against the
request working directory.

## Why

Each local query needs one access or metadata operation. Path ownership,
a second wrapper lookup and the suspended wrapper path did extra work
before reaching it. The numeric prefix of a stat result has a known
size and can be built as a packed array in one pass.

## Turned down

A blocking worker hop adds scheduling to a single syscall. A new stat
cache would need invalidation for PHP-visible filesystem changes.
