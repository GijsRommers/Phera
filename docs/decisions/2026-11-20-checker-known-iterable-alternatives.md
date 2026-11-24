# Report known scalar alternatives passed to foreach

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Report definitely scalar foreach subjects at level 3 and scalar alternatives
within a union at level 7. Apply the nullable and mixed thresholds separately.
Keep the false result of regex splitting distinct from the list of matches.
Preserve the documented false result of DNS lookups and Redis scans so a
false guard leaves an iterable.

## Why

A failed split can reach a loop. A guard on false handles the failure
and should narrow the remaining result to its iterable alternative.

## Turned down

Treating an unresolved object or a broad internal unknown type as a
definitely invalid foreach subject.
