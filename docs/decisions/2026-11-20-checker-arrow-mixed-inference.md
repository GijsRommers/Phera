# Infer precise arrow bodies under a mixed return declaration

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use a known arrow expression type as the returned value of an arrow
function declared to return `mixed`. Keep mixed when the expression
itself is mixed.

## Why

PHPStan can infer the result of `fn (): mixed => 1` when the arrow is passed
to a generic callback parameter. The declared type still allows any result.
