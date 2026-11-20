# Keep resolved includes in the worker cache

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Keep resolved include spellings on the worker and compiled revisions in a shared canonical-path cache. Revalidate positive lookups on the timestamp interval and retry unsuccessful lookups.

## Why

Hot requests can reuse the resolved file without queueing on a blocking thread. A failed lookup must allow the next file in PHP's search order to win.

## Turned down

A process-wide lock on warm lookups, or caching missing includes across requests.
