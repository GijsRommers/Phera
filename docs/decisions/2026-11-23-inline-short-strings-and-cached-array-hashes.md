# Store short strings inline inside the 64-byte string allocation, cache array hashes per string, and borrow keys on array reads

- Date: 2026-11-23
- Area: Speed and method
- Decided by: Design

## Decision

1. **`PhpString` stores strings up to 38 bytes inline inside `Rc<PhpStringInner>` alongside a lazily cached array hash (`Cell<u64>`).** `PhpString` stays 8 bytes (`Value` and `Option<Value>` remain 16 bytes; `Entry` 32 bytes; `PhpArray` 56 bytes). `PhpStringInner` is 48 bytes so `RcInner<PhpStringInner>` is exactly 64 bytes (one cache line and a power-of-two mimalloc bin). Strings over 38 bytes spill to `Vec<u8>`. Any mutation (`push_bytes`, `as_unique_bytes_mut`, `make_mut_bytes`, or copy-on-write clone) resets the cached hash to `0`.
2. **`PhpArray::Index` uses one process-wide randomized `OnceLock` seed instead of a per-array `RandomState`.** `Index` shrinks by 16 bytes, `Index::build` skips per-array seed initialization, string keys reuse their cached `PhpString::array_hash()` across every array in the process, integer keys use a seeded multiply-xor hash, and `OrderedMap::insert` probes the hash table once.
3. **Pointer-first string equality and zero-clone array reads.** `PhpString::eq`, `PhpString::cmp`, and `Value` string comparison short-circuit on `Rc::ptr_eq`. `PhpArray` exposes `get_str`, `get_int`, `get_mut_str`, `get_mut_int`, `contains_str`, and `contains_int` with an inline non-numeric fast path on the first byte; `FetchIndex`, `fast_property_elements`, `fast_local_elements`, `array_key_exists`, `in_array`, and `concat` (`PhpString::from_pair`) use them without cloning key handles or intermediate arrays.

## Why

On the Symfony and Laravel profiles, memory plumbing (`memmove`, `memcmp`, allocator, drop glue) dominates framework requests, and `PhpString(Rc<Vec<u8>>)` paid two heap allocations per short string, zero hash reuse across arrays, and `Rc::clone`/`Rc::drop` on every string-keyed array lookup.

An initial 32-byte `PhpStringInner` (`RcInner` = 48 bytes) regressed string-allocation benchmarks (`strtoupper`, `substr`, `explode`, `implode`) by ~96 retired instructions per string because `popcount(48) = 2` sent every `mi_malloc_aligned(48, 8)` through mimalloc's `_mi_theap_malloc_zero_aligned_at_overalloc` slow path. Sizing `PhpStringInner` to 48 bytes (`RcInner` = 64 bytes, `popcount(64) = 1`) both avoids that slow path and provides 38 bytes of inline storage inside the single cache-line allocation—matching `RESULT-values.md`'s conclusion that short-string bytes must not live inside `Value` itself.

Measured on Apple M4 against `origin/main` (`0c63b85f`, fat LTO, 5 rounds per CLI script; 6 alternating pairs per Laravel route):

- **Laravel `/api/posts?page=2`** (warm, 30 vs 110 reqs): `108.96M -> 107.60M` instructions/req (`-1.25%`, 6/6 pairs favored candidate).
- **Laravel `/api/hello`** (warm, 40 vs 190 reqs): `24.47M -> 24.43M` instructions/req (`-0.16%`).
- **`bench-framework-patterns.php`**: `-3.45%` instructions, `-3.26%` cycles.
- **18-key hashed string-map probe**: `-5.49%` instructions, `-4.83%` cycles (`1.04x` PHP 8.5).
- **`bench-calls.php`**: `-1.13%` instructions, `-4.65%` cycles (`2.97x` PHP 8.5).
- **`bench-natives.php`**: `-0.04%` instructions (eliminated the 48-byte overalloc regression).
- **`bench-objects.php`**: `-0.13%` instructions (`fetch_property_element` and `fetch_local_element` kept `#[inline(never)]` so `further_step` does not grow).

## Turned down

- **48-byte `RcInner<PhpStringInner>` (`Cell<u64>` + `Vec<u8>`):** hit mimalloc's aligned-overalloc slow path on every string allocation (+2.65% on `bench-natives.php`, +1.05% on `bench-calls.php`).
- **Checking cached hash inequality inside general `PhpString::eq`:** loaded two uncomputed `0` hash fields on short unhashed string comparisons (such as `in_array` over `explode` output).
- **Inline short strings inside `Value` itself:** previously rejected in `RESULT-values.md` because it makes every `Value` copy/move 40–70% dearer.
- **Inlining `fetch_property_element` / `fetch_local_element` into `further_step`:** grew `further_step` and regressed `bench-objects.php` by +1.25%.
