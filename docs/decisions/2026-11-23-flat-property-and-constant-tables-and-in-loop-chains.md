# Flat property and constant tables on `Class`, direct chained paths, and fused 0-arg calls

Date: 2026-11-23

## Context

Profiling Symfony + Doctrine (`/api/db-yardstick`) after Breakthrough 2 showed that:

1. **16,413 general-tier property/path instructions per request** (~117.5M instructions, >26% of the entire request) exited the interpreter loop because multi-step paths (`$this->_rsm->fieldMappings[$k]`, `$class->fieldMappings[$f]->type`, `$rows[$k]->prop`) and polymorphic sites bypassed the per-instruction `PropertyEntry` cache (`RefCell` borrow + `CachedSite` lookup + `WindowContext`).
2. **Class constant lookups** (`Class::CONST`, `self::CONST`, `$obj::CONST`) hashed strings into `HashMap<String, _>` on every in-loop miss or dynamic access.
3. **Constructorless `New` + `Call` and 0-arg `InitMethodCall`/`InitStaticCall` + `Call`** paid for pushing a pending call descriptor and re-dispatching the immediate `Call` instruction.
4. **mimalloc v3** over-allocated aligned allocations compared to **mimalloc v2**.

## Decision

1. **Data-oriented `PropertyTable` on `Class`**:
   - Intern case-sensitive property and constant names once per unit as compact `PropKey(u32)` keys in `UnitMetadata`.
   - Build a flat, immutable `PropertyTable` per `Class` in `ClassTable::build_method_tables()` (shared across forks, zero per-instruction heap cache or `RefCell`).
   - Store each declared property as a 16-byte `Copy` `PackedProp` (key, owner class index, slot index, packed visibility/hooks/readonly/set-visibility/class-type bitset, `TypeMask`). Use dense linear scan for classes with `<= 8` declared properties and a power-of-2 open-addressed table for larger classes, plus a compact private-shadow side slice for lexical private shadowing.
2. **Direct & chained in-loop property paths (`fast_properties.rs`)**:
   - Execute single-step and arbitrary chained property/index paths (up to 6 steps) directly inside the interpreter loop for `Fetch`, `SendPath`, `AssignPath`, `AssignPathFromLocal`, and `IncDecPath`.
   - Use a single-borrow zero-array-clone walk for `[Property, Index..]` paths and borrowed `FastKey` lookups (`get_int` / `get_interned` / `get_str`) with zero `PhpString` cloning.
3. **Flat `ConstantTable` on `Class`**:
   - Store declared class constants in a flat `ConstantTable` indexed by `PropKey(u32)` with packed visibility flags, eliminating string hashing on class constant reads.
4. **Fused constructorless `New+Call` and 0-arg method/static `Call`**:
   - When `New` targets a constructorless class and is immediately followed by `Call` in an unguarded frame, write the new object directly to `Call`'s destination slot and advance `ip + 1`.
   - When `prepare_php_method` targets a plain 0-required-parameter PHP method and is immediately followed by `Call`, push the frame and switch directly without an intermediate pending call entry.
5. **Enable `mimalloc` `v2` feature** in `Cargo.toml`.

## Measured Impact (fat-LTO release binary vs pre-push `main` `0c63b85f`)

- **Symfony + Doctrine `/api/db-yardstick`** (settled `YARDSTICK_WARM=300`, 2 alternating pairs, identical SHA-256 `c70dd487...`):
  - Pair 1: `439.68M -> 287.56M ins/req` (**-34.60%**)
  - Pair 2: `441.79M -> 291.32M ins/req` (**-34.06%**)
  - Mean: **`440.74M -> 289.44M ins/req` (-34.33%, -151.3M instructions/request)**
- **Laravel `/api/hello`**: `24.73M -> 23.84M ins/req` (**-3.63% ins / -4.67% cyc**)
- **CLI microbenchmarks** (vs `0c63b85f` and PHP 8.5 OPcache ON / JIT OFF):
  - `methods`: `18,936.56M -> 17,189.02M` (**-9.23% ins / -14.29% cyc**)
  - `string-map (18 keys)`: `967.27M -> 884.04M` (**-8.61% ins / -11.98% cyc**, **1.00x PHP 8.5**)
  - `objects`: `3,901.91M -> 3,589.75M` (**-8.00% ins / -18.30% cyc**, **1.94x PHP 8.5**)
  - `calls`: `5,816.61M -> 5,423.53M` (**-6.76% ins / -13.53% cyc**)
  - `natives`: `1,807.11M -> 1,721.73M` (**-4.72% ins / -12.22% cyc**, **1.46x PHP 8.5**)
  - `framework-patterns`: `8,252.49M -> 7,910.27M` (**-4.15% ins / -5.68% cyc**)
  - `properties`: `2,199.26M -> 2,123.64M` (**-3.44% ins / -1.54% cyc**, **1.98x PHP 8.5**)

## Safety & Verification

- 100% safe Rust (`#![forbid(unsafe_code)]`, verified by `.github/scripts/check-unsafe.py`).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and unit/oracle suites pass with 0 failures.
