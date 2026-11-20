# What a fork resets, and what is not state

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

A fork has its own superglobals and request time, an empty
header list and response status, empty output buffers (the buffers opened
before the call stay open, without their contents: output written before
the snapshot is never sent), no last error, the time limit in force at the
call with no time used, and fresh seeds for `rand()`, `mt_rand()` and
`random_int()`. `microtime()`, `hrtime()`, `uniqid()` and `getmypid()` read
the clock and the process, not request state: nothing to reset; all forks
of a worker report its process id, as all requests of a PHP-FPM worker do.
Object ids continue from the snapshot's: the first object a request makes
gets the same id in every fork, as in PHP-FPM where every request runs the
same boot. (`crates/vm/src/snapshot/copy.rs`)
