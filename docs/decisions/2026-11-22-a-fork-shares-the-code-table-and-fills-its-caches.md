# A fork shares the code table and fills its own caches from the snapshot's

- Date: 2026-11-22
- Area: Speed and method
- Decided by: Claude (design), for review by Fable

## Decision

A fork no longer copies what it will only read.

**Data a request reads and rarely changes** (the function facts, the loaded-file slots, the names of composed trait methods) is held by the snapshot and its forks together. `SharedTable` (`crates/vm/src/shared_table.rs`) is a reference-counted slice, `Rc<[T]>`: `table[i]` is one bounds check and one load, as with a `Vec`, with no test for sharing. The request that first changes a table (`CodeTable::load` of a file the boot did not load, a class it declares, a trait method it composes) copies it once, with room at the end, and owns the copy from then on. A write that would change nothing is not made: a request that declares a class the snapshot declared already writes the same declaring class and the same imported facts, and copies nothing. `trait_method_names` is a sorted flat array searched by method number (`Rc<Vec<(u32, Rc<str>)>>`) instead of a `HashMap<FunctionIndex, String>`: it is read only to name a function in an error or a trace.

**Caches a request fills as it runs** are not copied. The first copy of a file's cache (the boot's, which becomes the snapshot) keeps what the boot filled as `UnitCache::base`, without the entries that hold an object of the boot (`LinkedFunction::shared`, `Callee::shared`: only a link to a function, a native function or an intrinsic, a property entry, a method-site record qualify). A fork's `UnitCache` starts with every cell empty and the same `base`. A cell is made from the snapshot's at the moment it is first needed, on the path that was the miss already:

| cache | made from the snapshot's | when |
| --- | --- | --- |
| site cells of a function | the cells of that function | its first `site()` |
| method-site records | the file's flat array | its first method site |
| class checks | the file's array | the file's first stored answer |
| function symbol links | the one link | the first call of the symbol that finds none |
| native call links | the file's array | the file's first native call |
| closure signatures | the one signature | its first closure |

Nothing a request writes is in `base`: a request writes only into its own cells, and `base` is never changed. A cell that holds an object (a bound method, a closure) is never put in `base`; the fork copies none and makes it again. The snapshot's classes are shared by every fork, so the class addresses, class numbers and function numbers in `base` mean the same in a fork as they did in the boot.

The fork's leases are as before: it keeps the snapshot's alive.

## Why

Measured on 2026-11-21 on a booted Laravel application, 80 percent of what one fork allocated (2.79 MB) was the code table's own data and 2.4M to 3.2M machine instructions went to copying it and freeing the copy. On the synthetic application of `crates/runtime/examples/fork_cost.rs` (41 files, 3,700 functions, 328 classes, 200 objects), one fork before and after:

| | main (`78637583`) | this change |
| --- | ---: | ---: |
| bytes allocated per fork | 2,631,770 | 129,066 |
| allocations per fork | 4,627 | 820 |
| instructions per fork | 4,026,173 | 892,160 |
| time per fork (10,000 forks) | 513 to 523 µs | 75 µs |
| request that runs 4 of the 41 files, instructions | 6,157,127 | 3,315,226 |
| the same, and loads and declares 70 classes the boot did not load | 10,149,515 | 7,812,851 |

The request gate (`~/.phera-review/bin/gate.sh`, plain Laravel 13, 12 pairs, millions of instructions per request): `/api/hello` 31.847 to 30.779 (-3.35 percent), `/api/posts?page=2` 126.751 to 126.276 (-0.38 percent).

Bytes a fork allocated by structure (`PHERA_FORK_FOOTPRINT=1`, debug build, the same application):

| structure | was | is |
| --- | ---: | ---: |
| site cells | 1,064,640 | 0 |
| per-function site boxes | 69,792 | 656 |
| per-type class checks | 313,632 | 0 |
| per-symbol function links | 65,568 | 0 |
| closure signatures | 384,096 | 0 |
| function facts | 412,016 | 0 (shared) |
| loaded-file records | 11,976 | 12,792 (the file records; the slots are shared) |
| the fork in all | 2,395,598 | 87,326 |

What is left is PHP values (objects, arrays, reference cells) and the extension state map. What the fork costs per structure in instructions is not separable by a counter (they are one copy and one free of one object graph); in a profile of the fork alone the copying of the cells was 24.6 percent of the instructions, the facts and links and class checks in `fork_vm` 14.8 percent, dropping the site boxes 5.6 percent, and the allocator calls that went with them about 25 percent. After the change nothing of the code table is among the ten most expensive functions of a fork.

## Turned down

The three ways to treat the caches, measured on the synthetic application (instructions):

| | fork alone | request, 4 files | request, 4 files and 70 classes |
| --- | ---: | ---: | ---: |
| copy the snapshot's cells into every fork (main) | 4,026,173 | 6,157,127 | 10,149,515 |
| every fork starts empty | 988,587 | 4,553,384 | 9,608,077 |
| every fork starts empty and takes the snapshot's cells at first use (this) | 892,160 | 3,315,226 | 7,812,851 |

- **Starting every fork with empty caches**: the fork is as cheap, but every site, link and class check the request reaches is resolved again (a name lookup, a property resolution, a class derivation): 1.2M instructions more per request for 4 files than taking what the snapshot knows.
- **Sharing the snapshot's cells for reading and copying a file's cells at the first write**: the writes are made through `&UnitCache` by interior mutability in about ten places, some in the interpreter loop (`calls/fast_calls.rs`), so a first write cannot be intercepted without a test on every write, and a hit would need to ask which table to read. Filling the fork's own cell from the snapshot's on the miss, which exists already, has no test on a hit.
- **A shared base plus the request's additions for the facts** (a fork reads `base[i]` below the base's length and its own vector above): one comparison on every read of the facts, and the decision of 2026-11-20 measured 2 to 4.5 percent for a split table. Copying the whole table on the first change costs what every fork paid before (about 0.27M instructions on a Laravel `/api/hello`, about 1.4M on `/api/posts`, which loads and declares many classes), and nothing for a request that changes nothing. When the boot declares the classes the requests need, that copy goes away.
- **`Rc<Vec<FunctionFacts>>` with `Rc::make_mut`**: one more dependent load on every read of the facts. `Rc<[FunctionFacts]>` has the pointer and the length in the code table as the `Vec` had; what is left is that the data starts 16 bytes after the pointer, so an address is computed with a `lea` where an `add` from memory was enough. Measured on `bench-fib.php`: +0.43 percent (+3 instructions per call), the whole cost of the read.
- **Class checks made with the file (no test on a read)**: an array per file in every fork, 313,632 bytes and an allocation per file; the gate gave `/api/hello` -2.15 percent instead of -3.35 and `/api/posts` +0.21 percent (the facts table was also copied with placeholders that each allocated, since fixed), the 0.4M instructions of allocating, filling and freeing the arrays of every file against a null test on each read of a class check (two instructions, `known_class_match`). The test stays.
- **Making the boxes of each function's cells with the file**: the same trade; the box is made at the first `site()` of any function of the file.

## What it costs, and what is not covered

- Reads: the facts through `Rc<[T]>` (+1 instruction per read, above), the class checks and the function boxes (a test on the cell that holds them). Instructions of `benchmarks/bench-*.php`, 7 runs each, median of the new binary against the median of main: fib +0.43 percent, calls +0.18, closures +0.62, properties +0.61, function-objects +0.58, objects +0.56, hierarchy +1.02 (`peek` in `known_class_match`, two method calls with two class checks per iteration), typed-objects +1.11 (median; +0.84 from the lowest of 7, main itself spans 6,109 to 6,253 million), all others within 0.2 percent. Two of them are over the 1 percent of `docs/decisions/2026-11-18-a-change-must-not-make-any-benchmark-script.md`; they are the benchmarks that read the most class checks and property sites.
- A request that loads a file the boot did not load, or declares a class it did not, copies the facts table (and the slots) once. It is the cost the fork paid before, moved to the request that needs it.
- Tests: `a_forks_code_table_and_caches_are_its_own_when_it_changes_them` (`crates/runtime/tests/snapshots.rs`) runs forks one after the other and at the same time, each changing something another fork and the snapshot must not see: a class declared under a name the other requests declare differently (site cells, method-site records, class checks and property entries filled for another class), a file included with a function and a class that composes a trait method (facts, slots, trait method names), a function symbol the boot left unlinked that one request links and another does not, a closure bound to another class. The unit tests of `SharedTable` cover the copy on the first change. Without a test of their own: the native call links, and a snapshot that is replaced while forks of the old one are alive (the same path as before this change).
- `cargo test --workspace --no-fail-fast`: 893 passed and the same 16 failures as `~/.phera-review/baseline-326e7438/tests.outcomes` on this machine (PHP oracle and database tests that need the validation setup); phpt 6,918 passed, 0 failed.
