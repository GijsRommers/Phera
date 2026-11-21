# Report unnamed datagram peers as null

- Date: 2026-11-21
- Area: Streams
- Decided by: Design

## Decision

`stream_socket_recvfrom()` sets its address output to `null` when the
kernel reports no sender address. A named peer is reported with its
actual address.

## Why

The unnamed Unix-datagram peer has no path. PHP's output in this case can
contain bytes unrelated to an address. Phera only returns bytes supplied
by the kernel as an address.

## Turned down

Returning bytes from outside the reported address to match PHP's output
for an unnamed peer.
