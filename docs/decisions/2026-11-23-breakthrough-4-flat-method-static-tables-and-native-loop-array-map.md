# Breakthrough 4: Data-Oriented Flat Tables, Cache Deletion, and Native-Loop `array_map`

Date: 2026-11-23

## Context

Hardware LBR `perf` profiling of `origin/main` (`f9cf1e1f`) on Linux (`Hades`) exposed five clear structural bottlenecks:

1. **4.63% of Laravel `/api/posts?page=2`**: `ClassTable::native_methods` was never shared across forks, causing every request to rebuild `MethodTable` (and hash every method name through `name_keys::number` + SipHash) for native classes such as `PDO`, `PDOStatement`, and `DateTime`.
2. **15.74% inclusive `array_map::map_rows` / 25.91% `finish_callback`**: `Illuminate\Support\Arr::map()` invokes 2-array `array_map($callback, $array, array_keys($array))`, and Laravel also passes object-method (`[$this, 'method']`) and synchronous native (`'strtolower'`, `'trim'`) callables—all of which missed `native_loops.rs` (which previously only handled 1-array `Closure` callbacks).
3. **1.04% self in `<RandomState>::hash_one::<&TypeId>` + `9.11%` inclusive in `fork_vm`**: `RequestState` used `std::collections::HashMap<TypeId, Slot>` for 4–12 entries per request, and `Fork` / `name_keys` used SipHash-1-3 (`std::collections::HashMap`) without pre-sizing.
4. **38%+ of `bench-methods`**: `MethodTable` stored `Option<(NameKey, Rc<Method>, Weak<Class>)>` (pointer chasing + `Weak::upgrade()` on every private/protected call), and `prepare_static` called `ClassTable::by_key` + string `memcmp` on every `self::`, `static::`, and `parent::` call.
5. **Remaining per-instruction / per-unit / per-class caches**: `static_sites.rs` (`CachedSite::Static`), loop `property_sites` caches, `UnitCache::class_types` (`ClassTypes`), and `Class::declaration_matches: RefCell<HashMap<String, bool>>` added per-unit/per-class heap state and forced polymorphic class-typed parameters and `CallDirect` calls out of the interpreter loop.

## Decision

Following data-oriented design principles (flat arrays, integer indices, `Copy` descriptors, and deleting caches rather than adding them):

1. **Flat `StaticPropTable` and `ClassTypeRule` on `Class`; delete `static_sites.rs` and loop property site caches**:
   - Store static properties in a flat 16-byte `Copy` `StaticPropTable` on `Class` indexed by `PropKey(u32)` and resolve `self`/`parent`/`static`/named static property reads, writes, and array-element walks directly in `fast_static_properties.rs`.
   - Store compact 8-byte `Copy` `ClassTypeRule` + `NameKey` slices on `PropertyTable` (`allows_object_class`), unifying single and chained class-typed property writes and compound property assignments without `CachedSite::Property`.
2. **Delete `UnitCache::class_types` and `Class::declaration_matches`**:
   - Replace per-unit `ClassTypes` and per-class `declaration_matches` `RefCell<HashMap>` caches with direct zero-cache `fast_class_type_matches` (`NameKey` + `Class::derives_from` pointer walk) for single, union (`A|B`), and intersection (`A&B`) types across `enter_direct`, `accepts_on_second_look`, `leave_plain`, `leave_to_native`, `leave_to_native_loop`, `parameter_binding`, and `returns`.
3. **Flat 16-byte SoA `MethodTable`, worker-shared native `MethodTable`s, zero-SipHash `RequestState` / `Fork` / `name_keys`, fast `self`/`static`/`parent` keys, and zero-clone `[Index, Property]`**:
   - Pack `MethodTable` slots into 16-byte `Copy` `PackedMethod` entries (`NameKey`, declaring `ClassIndex`, `FunctionIndex` or side index, packed visibility/kind/static/abstract flags), keeping `Rc<Method>`/`Weak<Class>` only in a side slice for non-PHP methods.
   - Delete `ClassTable::native_methods` so native classes populate `class.method_table` lazily once on the worker-shared `Class` (zero per-request rebuilds, zero eager RSS for unused extensions).
   - Compare `self`/`static`/`parent` via pre-interned process `NameKey` constants before `ClassTable::by_key`.
   - Replace `RequestState`'s `HashMap<TypeId, Slot>` with a flat `Vec<(TypeId, Slot)>`, switch `Fork` and `name_keys` to `foldhash::HashMap`, and pre-size `Fork` maps from `classes.objects.len()`.
   - Borrow `&Object` directly out of arrays for `[Index, Property]` paths (`$items[$i]->key`) with zero `Rc::clone`/`Rc::drop`.
4. **Native-loop 2-array, object-method, and synchronous native `array_map` + zero-clone callback return**:
   - Extend `native_loops.rs` to handle 2-array `array_map($fn, $a, $b)` (`Arr::map`), non-autoload object-method (`[$obj, 'method']`) and function callables, synchronous native callbacks (`strtolower`, `trim`, etc.), class-typed closure parameters, direct PHP function/method callback frame entry, and `take`-based zero-clone `leave_to_native_loop`.

## Measured Impact (Linux `Hades`, fat-LTO `604e80fd` vs `origin/main` `f9cf1e1f` & PHP 8.5.13 OPcache ON / JIT OFF)

All 7,200 measured Laravel responses, 500 profiled responses, and 168 CLI invocations matched PHP 8.5.13 byte-for-byte.

### Laravel (per-request median across 4 rotating rounds of 200 verified requests)
| Route | `604e80fd` ins / cyc | `f9cf1e1f` ins / cyc | PHP 8.5.13 ins / cyc | Ratio vs PHP | Δ vs `f9cf1e1f` |
|---|---:|---:|---:|---:|---:|
| `/api/hello` | 20.99M / 16.60M | 22.83M / 17.08M | 5.96M / 5.31M | **3.52× / 3.13×** | **−8.06% / −2.82%** |
| `/api/posts?page=2` (L1 default) | 48.64M / 38.99M | 50.90M / 39.77M | 13.70M / 10.34M | **3.55× / 3.77×** | **−4.42% / −1.96%** |
| `/api/posts?page=2` (L2 `--warmup`) | 35.23M / 27.79M | 37.32M / 28.64M | 13.70M / 10.37M | **2.57× / 2.68×** | **−5.60% / −2.97%** |

### CLI Highlights (4-round median vs `f9cf1e1f`)
- `bench-natives`: **−23.24% ins / −24.66% cyc**
- `bench-object-sort`: **−8.23% ins / −11.49% cyc**
- `bench-methods`: **−6.68% ins / −2.41% cyc**
- `bench-typed-objects`: **−3.01% ins / −3.03% cyc**
- `bench-framework-patterns`: **−0.84% ins / −1.99% cyc**
- All 14/14 measured CLI benchmarks reduced instruction counts.

### Hotspot Profile Confirmation (`/api/posts?page=2`, 8,199 LBR samples)
- `MethodTable::try_init` (`4.63%` previously): **eliminated** (0.00% separate attribution).
- `array_map::map_rows` (`15.74%` inclusive previously): **eliminated** (0.00% separate attribution).
- `hash_one::<&TypeId>` (`1.04%` previously): **eliminated** (0.00% separate attribution).
- Follow-up fix in final commit: reverted eager pre-building of all 250+ unused native classes in `build_method_tables()` (retaining lazy once-per-worker sharing on `Class::method_table`) to eliminate the +5–8.8 MiB RSS overhead observed in the `604e80fd` run, and pre-sized `Fork` maps to eliminate `0.79% RawTable::reserve_rehash`.

## Safety & Verification

- 100% safe Rust (`#![forbid(unsafe_code)]`, verified by `python3 .github/scripts/check-unsafe.py`).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `phera-vm` / `phera-server` / `snapshots` / `phera-ext-standard` test suites pass cleanly.
