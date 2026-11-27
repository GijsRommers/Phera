# AOT Production Image (`.phb`) and Sound Closed-World Discovery

Date: 2026-11-27

## Context

Earlier design work (2026-11-23, at Breakthrough 7) proposed closed-world whole-program analysis, typed bytecode specialization, and pre-compiled `.phb` production images (`phera build`). Since then, `main` has landed Breakthroughs 8–13 (exception-safe `optimize.rs`, `UnitMetadata` process-wide `NameKey`/`PropKey` tables, VM & `CodeTable` recycling via `rollback_to_snapshot`, `PackedProp` property tables, `CallMethod`/`StaticCallCache`, `CallNativeDirect` + `GuardBuiltin`), and Breakthrough 14 (PR #242) introduces `crates/vm/src/fast_tier/` as Phera's 100% safe-Rust compact Tier-2 unboxed register VM (`ScalarLoop`, `ListLoop`, `IntRecFunc`).

## Decisions

1. **Analyze compiled bytecode, never PHPDoc.**
   `phera_bytecode::facts` defines compact, `Send + Sync`, zero-`Rc` fact tables (`SoundType`, `ProgramFacts`, `UnitBuildFacts`, `DynamicAudit`). Because `CompiledUnit` carries no PHPDoc, the rule "never trust unchecked PHPDoc for bytecode specialization" holds by construction. Every fact records the closed-world `Assumptions` (`SEALED_NAMES`, `SEALED_HIERARCHY`, `UNIT_LOADED`) it relies on.
2. **One unboxed Tier-2 executor.**
   Breakthrough 14's `fast_tier` supersedes a separate `run_typed` interpreter loop. Phase 2 will feed `ProgramFacts` directly into `fast_tier` (candidate bit, proven local types, elided entry guards, typed self-recursion) and add only high-ROI call/object-shape specializations (cross-file `CallDirect`, `ReturnUnchecked`, dead `GuardBuiltin` removal, `$this` property slots).
3. **Deterministic, hand-written `.phb` v1 binary format in `crates/aot`.**
   - 100% safe Rust (`#![forbid(unsafe_code)]`), no `serde`, no `mmap`.
   - 64-byte little-endian header with PNG-style magic (`\x89PHB\r\n\x1a\n`), format version, flags, `runtime_abi_hash` (covering format version, `BYTECODE_REVISION`, crate version, explicit 124-opcode ISA schema fingerprint, and combined web+cli `Registry` fingerprint), and whole-body BLAKE2b-128 checksum.
   - 7 sections: `MANIFEST`, `STRINGS` (single deduplicated byte/string arena), `FILES`, `CLASSMAP`, `UNITS` (directory + per-unit BLAKE2b-64 blobs decoded lazily into `OnceLock<Arc<CompiledUnit>>` using safe positional reads), `FACTS`, and `AUDIT`.
   - Explicit 16-byte instruction wire table generated from a single exhaustive macro: adding a new `Instruction` variant fails to compile until given a permanent opcode number.
   - Linear post-decode validator (`validate_unit`) verifies every register, target, table index, handler range, path, method site, and class reference before execution.
4. **Hybrid vs Strict runtime semantics with zero overhead when unused.**
   - `CodeCache` checks its optional `Arc<AotImage>` before disk compilation. When no `--image` is passed, execution is unchanged.
   - **Hybrid (default)**: Unchanged files serve pre-compiled bytecode from `.phb` (cached by `FileStamp` so timestamp revalidation does not re-read or re-hash files). Edited files or includes outside the image fall back cleanly to disk compilation.
   - **Strict**: Unknown includes fail as not found, stream includes throw `Phera\SealedImageException`, and both `eval()` and `class_alias()` (including dynamic `call_user_func('class_alias', ...)`) throw `Phera\SealedImageException` via a cold `ImageSeal` check in `RequestState` (adding zero fields to `Vm` and zero hot-path overhead) unless explicitly allow-listed (`--allow eval`, `--allow class_alias`).
   - Because the strict loader already confines all includes to units inside the `.phb` image and refuses stream includes, `dynamic_include` (such as Composer's `ClassLoader::includeFile`) is still audited and reported but does not fail strict builds or break `SEALED_NAMES` / `SEALED_HIERARCHY`. Allowing `class_alias` drops `SEALED_NAMES` while keeping `SEALED_HIERARCHY`; allowing `eval` drops both.
   - Non-entry, non-`autoload.files` files that fail to compile (such as optional vendor stubs) are recorded in `compile_failures` and omitted from the image rather than aborting the build; entry scripts and `autoload.files` remain hard errors.
   - An image is never picked up implicitly; `phera serve --dev` ignores `--image` with a warning.
5. **Known Phase 1 boundaries (queued for Phase 1 / PR 2 after PR #242 merges).**
   - Native class parent/interface edges and runtime-shipped bundled PHP classes (such as `Phera\Symfony\Runtime extends Symfony\Component\Runtime\SymfonyRuntime`) are supplied via `BuildConfig::native_supertypes` so sealed builds never falsely mark runtime-subclassed types as `EFFECTIVELY_FINAL`.
   - Dynamic calls to scope-mutating intrinsics (`extract`, `compact`, `get_defined_vars`) need a VM guard matching PHP's "Cannot call extract() dynamically" before Phase 2 optimizes on `LocalFlags::BY_NAME` / `MAYBE_REF`.
   - Bytecode flow analysis (`facts/flow.rs`), build-time real linker layouts/method tables, call/property resolution, and the debug-VM soundness oracle hook land in PR 2 once PR #242 merges.
6. **Phase 3 (Native AOT) direction.**
   For compute-heavy scalar/list kernels that need Go/C CPU speed under `#![forbid(unsafe_code)]`, **generated safe Rust** (compiling proven-scalar kernels to a `#![forbid(unsafe_code)]` crate linked with the runtime and dispatched via `fast_tier`'s `FastExit` protocol) beats Cranelift `.o` (which requires `unsafe extern` in Rust 2024 and leaves machine code unchecked by `rustc`). Implementation waits for a measured three-kernel prototype proposal approved by the owner.

## Consequences

- Phase 1 / PR 1 lands `crates/bytecode/src/facts.rs`, `crates/analyzer/src/sound/`, `crates/aot`, `crates/runtime/src/image.rs` + `CodeCache` image loading, `crates/vm/src/{eval.rs,class_alias.rs}` `ImageSeal` checks, and `phera build` / `phera serve --image` / `phera run --image` with zero merge conflicts against PR #242 (`fast_tier`) and zero hot-path overhead when no image is loaded.
- Cold startup on pre-built projects skips lexing, parsing, and compiling PHP source files. Strict images can serve pre-compiled bytecode even when PHP source files are removed from disk (note: PHP filesystem calls like `file_exists`/`is_file`/`realpath` still query the OS filesystem, so stripped deployments require classmap-authoritative autoloading or static includes, and images are bound to their OS and canonical project root).
