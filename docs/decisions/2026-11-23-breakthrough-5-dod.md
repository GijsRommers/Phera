# Breakthrough 5: Persistent SQLite Workers, Fast `readonly`/Dynamic Properties, and Zero-Alloc Leaf Array Forks

**Date:** 2026-11-23  
**Branch:** `t3/breakthrough-5-dod` (`fbebc25ad` vs `origin/main` `e6f235b80`)  
**Host:** Hades (Intel Core i7-11800H, Linux 7.2.15, rustc 1.99.1, fat LTO, PHP 8.5.13 FPM OPcache ON / JIT OFF)

## Summary

Four profile-driven, 100% safe-Rust (`#![forbid(unsafe_code)]`) data-oriented redesigns targeting the top post-Breakthrough-4 hotspots across Symfony 7.4.14 + Doctrine ORM 3.6.7, Laravel, and CLI benchmarks:

1. **Persistent idle SQLite worker pool (`crates/sqlite/src/connection.rs`)**:
   - Previously, every `Database::open()` spawned a new OS thread (`phera-sqlite`) + `tokio::runtime::Runtime` + `LocalSet` and tore them down on connection close/drop, abandoning mimalloc thread-local pages/segments on every SQLite-backed HTTP request (`mi_segment_abandon` 2.50%, `_mi_page_abandon` 2.31%, `mi_stats_merge_from` 2.72% on Symfony `/api/products` L1).
   - Replaced with a bounded pool of idle worker threads (`MAX_IDLE_WORKERS = 8`). Each open `Database` retains exclusive 1-thread ownership for its entire lifetime (zero deadlock risk, identical lock/transaction semantics), drops `LocalSet` and `Rc<Connection>` cleanly on close/drop, and returns the live thread + Tokio runtime + allocator heap to the idle pool.
   - Collapsed `mi_segment_abandon` **2.50% → 0.18%**, `_mi_page_abandon` **2.31% → 0.28%**, and `mi_stats_merge_from` **2.72% → 0.81%**.

2. **Fast-path `readonly` first-time init, asymmetric `set_visibility`, lossless `int -> float` widening, dynamic properties (`stdClass`/`#[AllowDynamicProperties]`), and `$obj->$name` (`crates/vm/src/classes/property_table.rs`, `crates/vm/src/calls/fast_properties.rs`)**:
   - Previously, `Machine::assign_property` and `Vm::assign_plain_property` rejected `readonly` properties, asymmetric `set_visibility` (`private(set)` / `protected(set)`), dynamic properties, and `PathStep::DynamicProperty(reg)`, forcing Symfony 7.4 + Doctrine 3.6 property writes through async `Vm::assign_path` → `plan_magic_mutation` → `validate_property_mutation` → `resolve_property_key` (**4.56% self** across those four symbols).
   - Packed 2-bit `set_visibility` into existing unused bits of `PackedProp::flags: u16` (0-byte growth), added `TABLE_ALLOWS_DYNAMIC_WRITE` / `TABLE_ALLOWS_DYNAMIC_READ` flags and O(1) `by_slot` lookup on `PropertyTable`, and handled cold dynamic paths out-of-line so the declared-property hot path stays untouched.
   - Collapsed the four Symfony slow-path symbols from **4.56% → 0.16% self** (−96.5%): `plan_magic_mutation` **1.58% → 0.05%**, `resolve_property_key` **1.28% → 0.04%**, `validate_property_mutation` **0.91% → 0.06%**, `assign_path` **0.79% → 0.01%**.

3. **Snapshot fork overhead reduction (`crates/value/src/fork/arrays.rs`, `crates/value/src/fork/objects.rs`, `crates/vm/src/calls/call_stack.rs`)**:
   - Added a zero-allocation fast path in `Fork::array` for leaf arrays (no nested `Value::Array` children), skipping `stack: Vec<Visit>` and `children: Vec<Rc<PhpArray>>` allocations on pure scalar/flat snapshot arrays.
   - Eliminated inserting non-incarnated objects into the duplicate `Fork::identities` map (`Fork::objects` already maps `address -> copy`).
   - Skipped recycled empty `FrameExtras` slots past `frames.len()` in `CallStack::fork_copy` (removing `FrameExtras::fork_copy` from the top 20).
   - Reduced `Vm::fork_vm` on Laravel `/api/posts?page=2` L1 from **5.79% → 4.52% self**.

4. **Synchronous `ReflectionClass::newInstanceWithoutConstructor`, auto-vivifying nested array writes, and `foldhash` in class & PDO tables**:
   - Converted `ReflectionClass::newInstanceWithoutConstructor` from `MethodEntry::public_async` (`Box::pin(async)`) to synchronous `MethodEntry::public` via `ReflectionEngine::reflect_new_instance_without_constructor`, accelerating `bench-reflection` by **−3.90% ins / −4.00% cyc** and Doctrine entity hydration.
   - Added `writable_creating` / `write_creating` in `crates/vm/src/element_writes.rs` so plain nested assignments (`$a[$k1][$k2] = $v`, `$this->m[$k][] = $v`) auto-vivify missing intermediate arrays inside the interpreter loop.
   - Switched class-table and PDO parameter maps to `foldhash`.

## Measured & Rejected Prototype Ideas (BT5 v1 → v2)

Before landing v2, we benchmarked a v1 prototype (`036cac9b7`) on Hades and **reverted three changes** that proved counterproductive:
- **16-byte inline `PhpString` SSO**: Although `Value`, `Option<Value>`, and `ArrayKey` remained 16 bytes via niche packing, nesting `Repr` inside `Value` turned `Value`'s flat 1-byte discriminant into a multi-level niche decode, regressing pure integer loops by **+8–11%** (`bench-arithmetic` +9.35%, `bench-function-array` +11.47%) and doubling `UnitCache::names` (+8–12% RSS).
- **`Object::class_index` + `ClassTable::class_of` indirection**: Replacing direct `object.class.as_any().downcast_ref::<Class>()` with an indexed lookup into `ClassTable::local` added pointer chasing and regressed `b_objects` (+8.10%) and `bench-object-sort` (+7.79%).
- **`Rc<SharedUnit>` / `Rc<SharedUnitCache>` indirection**: Added an extra pointer dereference on hot bytecode/constant reads without improving `fork_vm`.

## Verified Hades Linux Results (`fbebc25ad` vs `main` `e6f235b80` & PHP 8.5.13 FPM)

Two interleaved paired rounds (opposite commit order in round 2; 1 worker, 3 warmups, 200 sequential measured requests per run, idle-subtracted `perf stat`). All **11,126 HTTP responses** returned HTTP 200 and matched PHP 8.5.13 FPM SHA-256 byte-for-byte; all **138 CLI runs** matched PHP 8.5.13 CLI stdout SHA-256.

### HTTP Instructions, Cycles, and RSS

| Route | Layer | v2 ins/req | Main ins/req | Δ ins | v2 / PHP | v2 cyc/req | Main cyc/req | Δ cyc | v2 RSS KiB | Δ RSS |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Symfony `/api/products` | L1 | 28.491M | 35.745M | **−20.29%** | 4.735× | 23.381M | 27.101M | **−13.73%** | 94,890 | +1.46% |
| Symfony `/api/products` | L2 | 7.628M | 8.024M | **−4.93%** | **1.268×** | 7.046M | 7.291M | **−3.37%** | 101,442 | +5.12% |
| Symfony `/api/ping` | L1 | 12.510M | 12.773M | **−2.05%** | 3.900× | 10.016M | 10.200M | **−1.80%** | 74,760 | −0.28% |
| Symfony `/api/ping` | L2 | 5.875M | 6.047M | **−2.84%** | 1.832× | 5.360M | 5.347M | +0.24% | 75,800 | −1.52% |
| Laravel `/api/hello` | L1 | 20.352M | 20.847M | **−2.38%** | 3.414× | 16.225M | 16.493M | **−1.62%** | 127,782 | −2.41% |
| Laravel `/api/hello` | L2 | 15.870M | 16.314M | **−2.72%** | 2.662× | 12.335M | 12.475M | **−1.11%** | 122,482 | −0.25% |
| Laravel `/api/posts?page=2` | L1 | 47.341M | 48.502M | **−2.39%** | 3.456× | 38.078M | 38.496M | **−1.08%** | 138,050 | −0.86% |
| Laravel `/api/posts?page=2` | L2 | 34.379M | 35.060M | **−1.94%** | **2.510×** | 27.078M | 27.545M | **−1.70%** | 128,300 | −0.74% |

### CLI Summary (23 Benchmarks)
- **Instructions**: **22 wins / 1 loss** (`b_hooks` −4.41%, `bench-reflection` −3.90%, `bench-typed-objects` −1.00%; sole instruction delta `bench-properties` +0.52% while cycles improved −1.64%).
- **Cycles**: **17 wins / 6 losses** (median −0.50%; `bench-array` −11.49%, `bench-objects` −9.48%, `bench-reflection` −4.00%, `bench-object-sort` −3.11%, `bench-calls` −2.44%).
