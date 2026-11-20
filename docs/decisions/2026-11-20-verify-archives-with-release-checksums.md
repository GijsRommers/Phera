# Verify archives with release checksums

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

The workflow publishes one `SHA256SUMS` alongside the archives. The installer
and upgrader verify the exact archive entry before installing it.

## Why

A checksum from the same origin detects corrupted downloads. A compromised
release can contain both a modified archive and a matching checksum.

## Follow-up

Consider signing release artifacts with a verification key distributed
independently of the release.
