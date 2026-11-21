# Trace closure values during cycle collection

- Date: 2026-11-21
- Area: Cycle collection
- Decided by: Design

## Decision

A native class can supply a hook that visits the PHP values physically
owned by its payload. The existing iterative walk adds those edges
alongside declared properties and arrays. A payload that cannot be
inspected stays alive. WeakMap and WeakReference use the same hook with
an empty list of strong edges. WeakMap values still use conditional
edges.

Closures visit captures, closure-owned static cells and bound receivers.
A copied magic callable gets its own receiver handle. Static cells
held by an executing frame count as outside owners during that call.

The collector calls destructors before releasing any payload. It
detaches collectable payloads while it still holds the objects in the
garbage set, then drops the detached values.

## Further native classes

PHP 8.5.12 collects the following cycles when the final external
handle is unset.

| Native payload | Reproduction | PHP collection count |
|---|---|---:|
| ArrayObject | An object points to an ArrayObject whose backing array contains the object | 3 |
| SplObjectStorage | An object points to a storage object that holds it as a key | 2 |
| Generator | A suspended generator holds an argument that points back to it | 1 |

ArrayObject should expose its backing array or backing object.
SplObjectStorage should expose entry objects and their associated
values. Generator tracing also needs to expose a suspended frame and
arrange for its `finally` blocks to run before the frame is released.

## Known difference

The order of destructors within a garbage cycle can differ when object
IDs have been reused. PHP's order comes from its root buffer. Phera's
collector keeps its existing object-ID order.
