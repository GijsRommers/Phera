# A false level turned into an array is reported before its key, on every write route

- Date: 2026-11-24
- Area: General tier, array writes
- Decided by: Design

## Decision

A write through a path of keys (`$f['k'] = 1`, `$f[] = 1`, `$f['a']['b'] = 1`, `$alias = &$f['k']`) converts its keys level by level (`Vm::write_keys_of`), as compound assignments and property writes already did: each level is checked first (`Vm::check_write_container`: `false` becomes an array with PHP's deprecation; a scalar or an object without `ArrayAccess` cannot be indexed), then its offset is converted.

## Why

The plain write and the element reference converted all keys first, without looking at the levels, and the write turned a `false` level into an array silently: PHP's "Automatic conversion of false to array is deprecated" was missing for a local, a static property and the top level (it was given for property writes and `unset`). Reporting it where the write converts the level would come after the key's own diagnostics, while PHP reports the level first (`$f = false; $f[1.5] = 1;` gives the false-to-array deprecation, then "Implicit conversion from float 1.5 to int loses precision").

php-src: `Zend/tests/indexing_001.phpt`, `Zend/tests/offsets/appending_containers.phpt` and `Zend/tests/offsets/object_container_offset_behaviour.phpt` pass now; nothing else changes. Requests and benchmarks: see the pull request.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| the keys of a plain write or element reference | converted all at once (`Vm::keys`), levels unchecked | converted level by level with each level checked first (`Vm::write_keys_of`) |
| the level check of a write | inside `write_index_key` | `check_write_container`, also used for `[]`, which has no offset |
