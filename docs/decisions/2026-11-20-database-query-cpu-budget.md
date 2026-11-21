# Match PHP's database query cost

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Owner

## Decision

Bring Phera's database access to PHP's CPU cost or below.

## Why

The reference application spends that client-side cost about 49 times per
request, against a server that can answer in 0.09 ms.

## Turned down

Using similar request latency as the only performance measure. A server
round trip can hide extra client CPU work.
