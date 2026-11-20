# A multipart boundary is refused beyond PHP's own limit (5 KiB minus four bytes), the search for it is linear, and parsing stops when the request is abandoned

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

A multipart boundary is refused beyond PHP's own limit (5 KiB minus four bytes), the search for it is linear, and parsing stops when the request is abandoned.

## Why

A boundary of hundreds of kilobytes made parsing take minutes after the client had gone (security round 3). The linear search and the cancellation remove that; the limit itself only has to be PHP's.

## Turned down

RFC 2046's 70 bytes, the first choice: PHP accepts longer boundaries, and a request PHP answers must not fail on Phera.
