# Report the current realpath cache state

- Date: 2026-11-21
- Area: File metadata
- Decided by: Design

Phera canonicalizes file paths at the point of use. Its PHP realpath cache
is empty, so `realpath_cache_get()` returns an empty array and
`realpath_cache_size()` returns zero. The two calls leave file lookup
on its existing path and add no request state.
