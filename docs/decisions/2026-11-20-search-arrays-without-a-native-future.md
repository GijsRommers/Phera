# Search arrays without a native future

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Use the existing synchronous selector for strict array searches and for loose
searches whose operands can be compared in the value layer. Use ordered byte
equality for string pairs when either prefix rules out a numeric string.
Loose searches that may compare objects, including objects inside nested
arrays, retain the original comparison path.

## Why

Strict equality never calls PHP. Loose equality needs PHP for cases such as
comparing native objects or initializing a lazy object. Checking the value
kinds before selecting the handler keeps those comparisons in their original
order. Membership only needs values, so its synchronous scan reads no keys.

## Turned down

Adding per-array search metadata or indexing by value. An ordered scan is
enough to find the first match with PHP's comparison rules.
