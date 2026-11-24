# Preserve definitions associated with an assigned boolean guard

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

For a short-circuit expression assigned to a local, record the extra
variable definitions established on its true and false paths. Reapply
them when the local is tested. Clear the facts when their guard is
reassigned or the output binding is unset.

## Why

`$ok = $enabled && preg_match($pattern, $input, $parts) === 1` may skip
the match. When `$ok` is true, the call did initialize `$parts`.

## Turned down

Assuming that every read after a short-circuit expression can use its
conditionally initialized variables.
