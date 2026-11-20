# Keep pinned Composer inputs outside the checkout

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

The network comparison reads the original 25 manifests and locks from the
immutable commit that produced them and fetches each upstream source revision
into a temporary directory. The offline tests retain content-hash inputs,
expected hashes and the Composer semver results.

## Why

The lock files measure parity with specific dependencies. Fetching those same
files preserves that comparison and keeps large third-party inputs out of the
active tree. The compact offline data keeps every original assertion.

## Turned down

Regenerating the locked comparison from current registry metadata. The selected
versions could then change between runs.
