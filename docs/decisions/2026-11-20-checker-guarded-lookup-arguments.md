# Track the arguments of a guarded method lookup

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Track narrowing for repeated named method calls with literals, class
constants or direct local variables as arguments. Invalidate the fact
when a local used as an argument is reassigned or passed by reference.

## Why

A guard on one lookup supplies evidence about another lookup with the
same receiver and key. Distinct keys and arbitrary argument expressions
need their own evidence.
