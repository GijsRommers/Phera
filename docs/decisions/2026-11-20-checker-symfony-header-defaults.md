# Apply non-null defaults to configured Symfony header lookups

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the Symfony include to refine `HeaderBag::get()` when the supplied
default excludes null. Share the default-argument lookup with InputBag.

## Why

A header lookup with a non-null fallback always returns a value. The
Symfony PHPStan extension uses this guarantee at the call site.
