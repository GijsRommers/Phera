# Track mixed values on repeating while loops

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

At level 10, a callback result can make a variable mixed on a later
iteration. Recheck the body once when a continuing path proves new
mixed provenance for a definite variable. Keep the widened entry for
the remaining variables and use the first analysis to compute exits.
