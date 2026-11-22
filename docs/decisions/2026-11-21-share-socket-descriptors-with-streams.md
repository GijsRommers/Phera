# Share socket descriptors with streams

- Date: 2026-11-21
- Area: Sockets
- Decided by: Design

## Decision

Use the stream socket's reactor registration for `Socket` objects. An
imported or exported stream and its `Socket` use the same descriptor.
The descriptor stays nonblocking at the OS level. Blocking PHP operations
suspend until the descriptor is ready or the configured timeout expires.

A snapshot refuses to capture a live `Socket`. A closed `Socket` or an
`AddressInfo` can be copied into a fork.

## Why

Calls through a stream and through a `Socket` can operate on the same
connection with one reactor registration. Other requests can run on
the worker while PHP waits for socket I/O.
