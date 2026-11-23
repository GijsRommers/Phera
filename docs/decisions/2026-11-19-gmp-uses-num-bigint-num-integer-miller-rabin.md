# GMP uses `num-bigint`, `num-integer`, Miller–Rabin, and a request-local Mersenne Twister with GMP's seeding algorithm

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

GMP uses `num-bigint`, `num-integer`, Miller–Rabin, and a request-local Mersenne Twister with GMP's seeding algorithm.

## Why

Integer operations stay in Rust and seeded sequences remain reproducible.

## Turned down

A C GMP dependency or a generator that changes the seeded sequence.
