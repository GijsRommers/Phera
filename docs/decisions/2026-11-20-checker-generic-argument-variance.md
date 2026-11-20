# Compare generic arguments in their declared variance direction

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Compare the concrete arguments of an indexed generic class or interface.
Invariant parameters compare in both directions. Covariant and
contravariant parameters compare in their declared direction. Project
arguments through documented inheritance using the existing type
bindings. At levels 5 and 6, a reverse match leaves a directional
comparison uncertain. Level 7 checks the full direction.

## Why

A child can expose its generic arguments through a parent or interface
with different template names. The declaring class's variance governs
whether that projected type satisfies the expected contract.

## Turned down

Following unknown template arguments as though their bounds were concrete
arguments. This decision covers declaration-site variance in type
compatibility. Validating variance at declarations and parsing call-site
variance annotations require separate rules.
