# Preserve literal false for built-ins that signal failure

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the PHP 8.5 literal false return for the indexed APIs whose committed
stub table widened it to bool. A strict comparison can then remove that
failure arm.

## Why

After `if ($image === false) { throw new RuntimeException(); }`, the
result of an image factory is a `GdImage`.

## Turned down

Treating false as the only possible boolean result from every built-in.
