# Bind imported PHPDoc aliases from collected declarations

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Resolve PHPStan and Psalm class-level type imports after collecting their
source classes, including classes loaded through Composer. Support renamed
imports and aliases built from another import. Rewrite annotated types
with the existing substitution pass and keep unresolved imports uncertain.

## Why

The same imported alias must affect a declaration, a method body, and
call-site checking. Binding it after indexing keeps the result independent
of file order.

## Turned down

Executing PHP to discover aliases or changing the local approximation of
aliases in unrelated declarations when adding import resolution.
