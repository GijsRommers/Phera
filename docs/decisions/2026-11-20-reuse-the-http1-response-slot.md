# Reuse the HTTP/1 response slot

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Put the response slot on the HTTP/1 connection and reuse it when the
connection starts its next request. The response body gets its own
channel when PHP starts streaming. An HTTP/2 stream owns its slot
because requests on that connection can overlap. A slot's generation
identifies which request can publish through it.

## Why

HTTP/1 has one response in flight at a time. The response routes
ordinary bytes through a full body, and PHP uses the same output
dispatch for captured, terminal, and web writes. Pending HTTP output
does not allocate a body channel until it is actually streamed.

## Turned down

A separate response-slot allocation for every HTTP/1 request.
