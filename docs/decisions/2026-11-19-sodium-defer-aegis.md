# Leave AEGIS functions absent pending a pure Rust audit

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Register the PHP compatibility constants. Leave the six AEGIS functions absent.

## Why

The aegis crate’s default backend uses C. A published audit covering its pure Rust backend
could not be verified.

## Turned down

Adding an unaudited backend or advertising functions with placeholder results.
