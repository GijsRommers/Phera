# Iterate values for loose membership

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

The asynchronous `in_array` path walks values using the same comparison as
`array_search`. Only the key-returning search visits the keys.

## Why

The shared search built a key for each attempted comparison in a map.
Membership reports a boolean. A loose search in an eight-entry map with a
reference took 5,498 retired instructions before and 5,326 after.

## Turned down

The comparison can run PHP on some values. Keep that behavior in the
asynchronous path for both functions.
