# Publish HTTP headers in the caller scope

- Date: 2026-11-21
- Area: HTTP streams
- Decided by: Design

The VM replaces the caller's named variable through its existing symbol
slots. The request header API and the local variable share the original
array until one is changed. Closing or clearing the request API's copy
leaves any local value available, and an old alias keeps its old binding.
