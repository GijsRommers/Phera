# A change must not make any benchmark script more than 1 percent slower in instructions

- Date: 2026-11-18
- Area: Speed and method
- Decided by: Design

## Decision

A change must not make any benchmark script more than 1 percent slower in instructions. The destructors work has a stated exception: no script more than 2 percent, at most three above 1 percent, the sum not slower, every overage named.

## Why

The interpreter loop is sensitive to layout; small costs add up unseen. Correct object lifetime has a real price.

## Turned down

No rule; a rule without exception.
