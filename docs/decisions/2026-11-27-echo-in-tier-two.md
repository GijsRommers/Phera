# Echo runs in Tier 2

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

## Decision

`echo` of a string in a Tier-2 function no longer leaves Tier 2. The bytes
go into `FastStack::echo`, and the VM writes them to the request's output as
soon as the interpreter loop hands back, before it runs anything else. The VM
opens that buffer for a run only when writing the bytes later, in one piece,
is the same as writing each at once: the write lands in an `ob_start()`
buffer without a chunk size and no output handler is running (a handler runs
when the buffer is flushed, not on a write), or output has started and is not
flushed on every write. No operator diagnostic may be waiting and no
debugger may be capturing output. Any other operand, or a closed buffer, is
a structural deopt, as `echo` was before: it does not count towards turning
Tier 2 off for the function.

The VM's own `echo` of an interned string writes its bytes without making a
`PhpString` first.

## Why

`mandel` of `Zend/bench.php` prints 7,701 characters per run from its outer
loops: each left Tier 2, stored the frame back to the VM and entered again.

## Turned down

- **Passing the output buffer into Tier 2.** The loop does not own the
  output state (buffers, web response, sinks); a byte buffer the VM drains
  keeps that state in one place.
