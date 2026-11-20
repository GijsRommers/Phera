# Request PostgreSQL text on the worker

- Date: 2026-11-20
- Area: PostgreSQL driver
- Decided by: Design

## Decision

Own one PostgreSQL socket per connection and await it on the PHP worker.
Build on the existing `postgres-protocol` encoders, decoders and SCRAM
implementation. Use text result formats in Bind. Write the resulting bytes
into PDO's flat cell buffer according to PHP's rules for the column type.
Use the same result reader for pgsql, where cells stay as server text.

Previously, tokio-postgres ran a socket task and returned binary rows to the
caller over channels. PDO and pgsql sent another statement to recover the
server's text, first for every row and then for each group. The new result
reader gets the text in the original result. The server still applies its
session settings to the output.

## Protocol and session behavior

Keep session startup and resets, typed column metadata, named and unnamed
statements, streaming results, portals, COPY, notices, notifications and
error fields. Combine Parse, Bind, Describe, Execute and Sync wherever PHP's
observable prepare and execution behavior permits it. Preserve cursors and
existing refusal cases. A request interrupted with messages in flight
closes the socket. An explicit protocol cancellation uses a separate
connection and its backend cancellation key.

Reuse Phera's TLS policy. A DSN that requires TLS fails when encryption is
unavailable. Use the protocol crate to verify the SCRAM server-final
signature. Advertise channel binding only for a connection that supplies the
required TLS binding data. Check wire lengths and counts before allocating
or slicing. Cap an individual message at 64 MiB, including its length field,
and retire the connection if its framing is invalid.

## Verification

Start with a scripted peer for fragmented and coalesced frames, supported
authentication, failed SCRAM verification, an error partway through a
result, truncated messages, and a peer closing the socket. Run the PDO and
pgsql PHP oracles, live pool and TLS tests, bootstrap and cancellation tests,
and the php-src PDO PostgreSQL and pgsql suites. Check round trips against PHP
before CPU cost, including the first prepared execution, parameterized pgsql,
the 500-row fetch, an insert in a transaction, and an unbuffered cursor.

## Alternatives considered

SQLx and xitca-postgres request binary results on prepared statements.
`pgwire` can supply text from the worker's socket. Its prepared helpers
use multiple exchanges, and COPY and notification integration still require
a caller-owned protocol state machine. The existing protocol dependency
already supplies the codecs and authentication primitives Phera needs.
