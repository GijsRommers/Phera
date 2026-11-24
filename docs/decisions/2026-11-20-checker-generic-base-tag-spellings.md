# Normalize inherited template tag spellings

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Read `@template-extends`, `@template-implements`, and `@template-use` through
the same parser as their shorter PHPDoc spellings.

## Why

Doctrine Collections uses these tags to carry its key and value templates
to inherited interfaces. Callers of inherited methods need those types.
