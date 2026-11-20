# Serve cleartext HTTP/2 on the same listener

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Use Hyper's prior-knowledge HTTP/2 support on the existing listener.
Peek at the protocol preface within the receive deadline, then hand the
socket to Hyper's HTTP/1 or HTTP/2 parser. A partial preface is read and
replayed only if more bytes are needed to tell the protocols apart.
The accepted request runs on the same worker as its connection, and the
same streamed body feeds HTTP/1.1 chunks or HTTP/2 DATA frames.

## Why

A proxy can use a multiplexed upstream connection and receive the same
streaming response without changing the application. Hyper already owns
HTTP parsing and flow control. An ordinary HTTP/1 request reaches the
HTTP/1 parser without copying its prefix into a second receive buffer.

## Turned down

Writing HTTP/2 framing in Phera and adding a separate HTTP/2 listener.
The built-in listener uses cleartext prior knowledge. A proxy can
terminate TLS in front of it.
