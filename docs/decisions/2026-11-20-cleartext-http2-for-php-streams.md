# Serve cleartext HTTP/2 on the same listener

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Use Hyper's prior-knowledge HTTP/2 support on the existing listener. Read
the protocol preface within the receive deadline. The accepted request
runs on the same worker as its connection, and the same streamed body
feeds HTTP/1.1 chunks or HTTP/2 DATA frames.

## Why

A proxy can use a multiplexed upstream connection and receive the same
streaming response without changing the application. Hyper already owns
HTTP parsing and flow control.

## Turned down

Writing HTTP/2 framing in Phera and adding a separate HTTP/2 listener.
The built-in listener uses cleartext prior knowledge. A proxy can
terminate TLS in front of it.
