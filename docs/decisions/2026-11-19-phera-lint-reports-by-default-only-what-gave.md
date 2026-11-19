# `phera lint` reports by default only what gave no false finding on the Symfony and Laravel code bases; unused variables, unused private properties, switch fall-through and self-comparison are among the defaults

- Date: 2026-11-19
- Area: Tools
- Decided by: Design

## Decision

`phera lint` reports by default only what gave no false finding on the Symfony and Laravel code bases; unused variables, unused private properties, switch fall-through and self-comparison are among the defaults. Style, naming and complexity rules are opt-in.

## Why

A linter that cries wolf gets switched off.

## Turned down

All rules on by default.
