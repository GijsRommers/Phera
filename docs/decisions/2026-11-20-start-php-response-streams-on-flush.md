# Start PHP response streams on flush

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Keep ordinary PHP output in the existing response buffer. An explicit
`flush()` commits headers and opens a streamed body. Implicit flushing
and an explicit PHP buffer flush also start the stream when their output
reaches the transport. PHP's outer buffers retain their contents until
they release them.

## Why

A script can send partial output through its existing framework API. The
server passes an unknown-length body to Hyper with one queued chunk of at
most 64 KiB. Hyper selects the framing and slow clients suspend the
producer. Native functions that read and emit a series of chunks can wait
at the same output boundary before reading more. Ordinary responses send
their exact length with no chunk queue.

The installed PHP development server releases transport-bound output when
an output buffer is explicitly flushed. PHP-FPM can hold those bytes until
its transport is flushed. Phera sends those bytes promptly on either HTTP
version. Empty transport flushes commit the headers immediately.

## Turned down

Automatically streaming all output and holding a body channel for every
request. Applying whole-response compression to a stream would delay its
chunks, so server-side compression keeps its existing buffered path.
