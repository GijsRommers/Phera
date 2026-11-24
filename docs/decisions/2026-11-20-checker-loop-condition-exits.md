# Keep the effects of false conditions on loop exits

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the false condition scope when the checker sees a reachable condition
exit. Keep the true scope as a fallback when a later exit is possible and
the first walk has no false condition scope.

## Why

An assignment performed while deciding to leave a loop survives the
loop. Merging in the true condition scope can lose that assignment when
a short-circuit branch skipped it on the way into the body.

## Turned down

Treating every successful condition evaluation as a possible loop exit.
