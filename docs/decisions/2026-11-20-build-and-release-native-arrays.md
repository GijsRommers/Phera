# Build native arrays in their result storage

- Date: 2026-11-20
- Area: Value layout and built-ins
- Decided by: Design

Build packed results by filling the vector of live slots directly. A fill
with consecutive nonzero keys starts as a map with its final entries, so the
index can be built once. Builders whose keys can repeat still insert in
source order to replace values without moving their keys. Size hints on
those builders also carry through a conversion from packed to keyed storage.

Release the live slots in order on the first traversal. Nested ownership
still uses the iterative work list, and values written through mutable slots
use the same release path. This preserves destructor order and keeps deep
arrays off the Rust call stack.

The interpreter owns literal construction and the array construction around
PHP callbacks in `array_map`.
