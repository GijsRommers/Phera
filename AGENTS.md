# Phera Agent Guidelines

## Non-Negotiable Rules

1. **100% Safe Rust**: `#![forbid(unsafe_code)]` workspace-wide. Never introduce `unsafe` (even behind `#[cfg(...)]`). Verify with `python3 .github/scripts/check-unsafe.py`, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
2. **100% PHP 8.5 Parity**: Verify runtime behavior against real PHP 8.5 (`PHERA_PHP`). Preserve oracle tests, `.phpt` parity, destructor ordering, reference/COW semantics, and exact error/exception behavior. Never add benchmark-specific cheats — only general compiler, VM, and runtime optimizations.
3. **Zero Regressions Across Workloads**: Any performance change must be measured on real workloads (CLI, `Zend/bench.php`, and HTTP routes including `/plaintext`, `/json`, `/db`, `/fortunes`) with hardware counters (`perf stat`) where relevant. Keep Tier-1 hot loops (`Machine::run`, `fast_calls.rs`) free of cold branches and I-cache bloat.

## Data-Oriented Design Principles

Apply these principles aggressively when designing or optimizing hot data structures, VM internals, bytecode, arrays, and servers:

- **Flat arrays & dense integer indices over hash tables and pointers**: Replace hash maps and pointer-chasing graphs with flat contiguous `Vec`/slice tables indexed by compact integer IDs (`u16`/`u32`). Pack frequently checked flags or metadata into spare upper bits of indices/handles where appropriate.
- **Structure of Arrays (SoA) over Array of Structures (AoS)**: Separate hot fields from cold fields (and tags from payloads, e.g. parallel `tags: [u8]` and `bits: [u64]` register windows) so tight loops stay cache-line dense and branch cleanly.
- **Redesign instead of caching as a band-aid**: Caching is often cope for a slow underlying layout or redundant per-request work. Before adding a cache, ask whether the data layout or ownership boundary can be redesigned so the operation is inherently cheap — and delete obsolete caches once the underlying path is fast.
- **Stack-allocated / inline-capacity buffers over heap lists**: Replace short-lived or small bounded heap-allocated `Vec`s with stack-allocated fixed arrays or inline-capacity lists (`SmallVec`) to eliminate allocator traffic and pointer indirection.
- **Bitsets over arrays of `bool`**: Never store `Vec<bool>` or `[bool; N]` for dense flag sets; pack flags into `u64` / `[u64]` bitsets for 8× better density and single-instruction bit tests.
