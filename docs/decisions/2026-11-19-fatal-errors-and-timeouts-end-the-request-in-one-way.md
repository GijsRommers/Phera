# Fatal errors and timeouts end the request in one way

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

After a fatal error, including the execution time limit, every
object alive is marked as destructed; shutdown functions run; objects they
create get destructors in the normal end-of-request phases. A suspended
fiber is destroyed (unwound) like an object with a destructor: when its last
reference goes, or in those phases, never after a fatal error.
(`crates/vm/src/request_end.rs`, `crates/vm/src/fibers/lifecycle.rs`)

## Why

That is PHP's behaviour, and one path serves every fatal error.
The time limit's own handling of objects created after the timeout (from
#86) did the same job and was removed.

## Turned down

Running no destructor at all after a fatal error (PHP does run
those of objects the shutdown functions create); closing all suspended
fibers before the destructor phases (PHP destroys them in the phases'
order).
