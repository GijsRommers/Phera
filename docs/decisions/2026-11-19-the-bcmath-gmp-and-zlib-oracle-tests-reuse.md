# The BCMath, GMP and zlib oracle tests reuse the release binary in the hosted end-to-end job

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

The BCMath, GMP and zlib oracle tests reuse the release binary in the hosted end-to-end job.

## Why

The ordinary crate jobs have no CLI binary for the comparisons.

## Turned down

Building a second release binary in the crate jobs.
