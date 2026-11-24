# Use non-empty array facts for foreach entry

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

On the true path of an array truthiness test, mark the array non-empty.
Preserve that fact through array reversal and use it when checking whether
a foreach can skip its body.

## Why

After `if ($items) foreach (array_reverse($items) as $item) { ... }`,
the body runs at least once along the true path.

## Turned down

Applying the same inference to an iterable object. A truthy object may
yield zero elements.
