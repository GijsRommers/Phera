# Prepare all package archives before updating vendor

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Check, download and unpack locked packages into the cache in parallel. Start
materializing the prepared packages only when all of them are available.

## Why

A missing package, failed checksum or unsafe archive leaves the existing
vendor contents intact. The install still extracts and materializes packages
on multiple threads.

## Turned down

Updating the vendor tree as each download finishes. A failure in a later
download would leave only some of the requested package versions installed.
