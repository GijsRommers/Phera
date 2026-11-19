# No `unsafe` in Phera's own crates; a workspace lint forbids it and CI checks it

- Date: 2026-11-17
- Area: Safety and dependencies
- Decided by: Owner

## Decision

No `unsafe` in Phera's own crates; a workspace lint forbids it and CI checks it. Where a design seems to need it, the answer is a safe design that may be slower. A specific use may be proposed to the owner for a gain of about ten times; it is never written without his decision.

## Why

The owner's rule.
