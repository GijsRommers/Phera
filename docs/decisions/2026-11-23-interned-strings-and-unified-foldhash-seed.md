# Interned Strings and Unified Foldhash Seed

## Decision

Add `Value::InternedStr(&'static InternedStr)` and `ArrayKey::Interned(&'static InternedStr)` backed by a bounded, safe-Rust process-wide intern table (`Box::leak`, capped at 131,072 entries / 16 MiB / 4,096 B max string), paired with a single process-wide `OnceLock<foldhash::fast::RandomState>` for `PhpArray` hash indices.

Compile-time string constants, class/property names, mangled visibility keys, and array-literal string keys are interned once at load/link time. Dynamic runtime strings stay on `Value::String(PhpString)` (`Rc<Vec<u8>>`). Both `Value` and `Option<Value>` remain 16 bytes, `ArrayKey` remains 16 bytes, and `#![forbid(unsafe_code)]` is preserved across the entire workspace.

## Why

1. **Zero `Rc` traffic on constant strings and keys**: Copying a compile-time string, class name, or property key becomes a 16-byte bitwise copy with no refcount increment/decrement and no `Drop` glue.
2. **Precomputed hash and pointer equality**: Each `InternedStr` caches its `foldhash` hash, numeric-integer key parse (`is_numeric` + `numeric`), and PHP truthiness once. Comparing two interned strings or keys is a single pointer comparison (`std::ptr::eq`), and looking up an interned key in any `PhpArray` uses its precomputed hash directly without hashing bytes or allocating an `ArrayKey`.
3. **Unified `foldhash` seed + single-probe insert**: Sharing one `OnceLock<foldhash::fast::RandomState>` across all arrays lets `InternedStr::array_hash()` match every `PhpArray`'s hash table directly (shrinking `Index` by 16 bytes as well) and keeps `foldhash` speed for dynamic strings. `OrderedMap::insert` now updates or inserts in a single hash probe.

## Measurement

Measured on Apple M4 (`fat` LTO release build of `phera-cli`, compared against `origin/main` `0c63b85f`, and PHP 8.5 with OPcache enabled and JIT disabled):

- **Symfony + Doctrine `/api/db-yardstick`** (`symfony-yardstick.py gate --pairs 2 --requests 50`, matching response SHA-256):
  - `459.05M → 441.98M` instructions/req (**-3.72%**, -17.1M ins/req)
  - `133.96M → 128.12M` cycles/req (**-4.36%**)
  - `34.90ms → 32.50ms` CPU ms/req (**-6.88%**)
- **CLI microbenchmarks** (`bench_compare.py`):
  - `string-map (18 keys)`: `966.61M → 912.37M` ins (**-5.61%**), `162.76M → 155.27M` cyc (**-4.60%**), `1.05x` PHP 8.5
  - `framework-patterns`: `8229.32M → 7945.32M` ins (**-3.45%**), `1543.07M → 1469.87M` cyc (**-4.74%**)
  - `calls`: `5803.05M → 5736.42M` ins (**-1.15%**), `1090.61M → 1083.03M` cyc (**-0.70%**)
  - `methods`: `18891.31M → 18810.99M` ins (**-0.43%**), `3393.75M → 3305.21M` cyc (**-2.61%**)
- **Laravel routes** (`bench_routes.py`, 6 pairs):
  - `/api/hello`: `24.44M → 24.36M` ins (**-0.34%**), `8.49M → 8.38M` cyc (**-1.36%**)
  - `/api/posts?page=2`: `108.53M → 108.57M` ins (`+0.04%`, flat)
