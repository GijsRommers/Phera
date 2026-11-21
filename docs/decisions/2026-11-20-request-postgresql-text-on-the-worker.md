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
closes the socket, as it did with the pool's interrupted-connection policy.

Reuse Phera's TLS policy. A DSN that requires TLS fails when encryption is
unavailable. Use the protocol crate to verify the SCRAM server-final
signature. Prefer SCRAM-SHA-256-PLUS when TLS and the server offer it. Hash
the exact leaf certificate with its signature hash, substituting SHA-256
for MD5 and SHA-1. Apply `channel_binding` and `require_auth` before replying
to any authentication challenge. With no method restriction, the server
chooses the authentication method.

Check wire lengths and counts before allocating or slicing. Cap an
individual message at 64 MiB, including its length field. Notices and
notifications each keep up to 4 MiB and drop their oldest entries to make
room. Limit the parameter map to 128 entries and 128 KiB. Retire a socket
that exceeds the parameter limit or sends invalid framing.

Keep the full SQL or reject it. The PostgreSQL encoder rejects a NUL in PDO
SQL and in emulated SQL before writing a message. Native text binding sends
the full value, which PostgreSQL rejects as invalid text. Both paths keep the
connection usable. Legacy `pg_prepare` names follow PHP's NUL termination;
the prepared SQL sent for such a name is unchanged.

## Verification

Start with a scripted peer for fragmented and coalesced frames, supported
authentication, failed SCRAM verification, an error partway through a
result, truncated messages, and a peer closing the socket. Run the PDO and
pgsql PHP oracles, live pool and TLS tests, bootstrap and cancellation tests,
and the php-src PDO PostgreSQL and pgsql suites. Check round trips against PHP
before CPU cost, including the first prepared execution, parameterized pgsql,
the 500-row fetch, an insert in a transaction, and an unbuffered cursor.

The performance gate for this change is the query exchange count. Small reused
queries still cost more CPU than PHP. The profile is dominated by socket
work, and the same `by_id` measurement attributes about one extra BSD system
call per query to Phera on both PostgreSQL and MySQL. Changes to how workers
wait on sockets will be examined separately after the Doctrine benchmark.

## Alternatives considered

SQLx and xitca-postgres request binary results on prepared statements.
`pgwire` can supply text from the worker's socket. Its prepared helpers
use multiple exchanges, and COPY and notification integration still require
a caller-owned protocol state machine. The existing protocol dependency
already supplies the codecs and authentication primitives Phera needs.
