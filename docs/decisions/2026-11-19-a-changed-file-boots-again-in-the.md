# A changed file boots again in the background

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

A snapshot remembers the files its boot included and their
modification times. They are checked as opcache checks includes
(`opcache.validate_timestamps`, at most every `opcache.revalidate_freq`
seconds, 2 by default; 0 checks on every request). A change starts a new
boot on that worker in the background; requests keep forking the old
snapshot until the new one replaces it. (`crates/runtime/src/snapshots.rs`)

## Why

The reviewer's rule: never block requests on a re-boot. The same
two settings as includes, so one configuration covers both.
