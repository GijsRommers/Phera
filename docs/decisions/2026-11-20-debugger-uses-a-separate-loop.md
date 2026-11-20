# The debugger uses a separate interpreter loop

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

After a DBGp client connects, a request selects a separate interpreter path.
That path can publish frames at a source line and await the client. The plain
instruction loop and the destructor-only loop keep their existing dispatch.

Each worker chooses its HTTP handler at startup. With debugging disabled,
the ordinary handler uses the runtime specialization without debugger
configuration, trigger checks, or a pausable deadline.

## Why

An unattached request needs no breakpoint checks. Previous measurements
found instruction layout changes under fat LTO can affect even unrelated
programs, so release measurements cover both scripts and HTTP requests.

## Turned down

A breakpoint check in the plain loop.
