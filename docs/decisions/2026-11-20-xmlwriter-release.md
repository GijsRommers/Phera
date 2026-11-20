# Close XMLWriter URI output through the object release queue

- Date: 2026-11-20
- Area: Runtime
- Decided by: Design

## Decision

An XMLWriter with a destination registers for native release. The runtime drains it asynchronously
when the last PHP reference goes away. The URI file is owned by the writer, while a caller's
stream stays open if its resource is still referenced.

## Why

PHP makes pending XML bytes visible before the next statement after releasing a writer.
Using the existing object release queue keeps filesystem writes asynchronous.

## Turned down

Writing inside Rust's synchronous Drop or making every XMLWriter call flush its output.
