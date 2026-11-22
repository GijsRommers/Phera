# A fork starts with empty caches and fills them from the snapshot's, and the site cells are one table

- Date: 2026-11-22
- Area: Speed and method
- Decided by: Claude (design), for review by Fable

## Decision

A fork no longer copies the caches the boot filled, and the read path of the interpreter is the one of main (or shorter).

**Caches a request fills as it runs** are not copied. The first copy of a file's cache (the boot's, which becomes the snapshot) keeps what the boot filled as `UnitCache::base`, without the entries that hold an object of the boot (`LinkedFunction::shared`, `Callee::shared`: only a link to a function, a native function or an intrinsic, and a property, static or method-site entry qualify). A fork's `UnitCache` starts with every cell empty and the same `base`. A cell is made from the snapshot's at the moment it is first needed, on the path that was the miss already:

| cache | made from the snapshot's | when |
| --- | --- | --- |
| site cells of a function | the cells of that function (`CodeTable::base_sites`) | its first `CodeTable::site()` |
| method-site records | the file's flat array | its first method site |
| class checks | the file's array | the first typed parameter or return value of the file that reaches the slow path |
| function symbol links | the one link | the first call of the symbol that finds none |
| native call links | the file's array | the file's first native call |
| closure signatures | the one signature | its first closure |

**The site cells are one table in the code table**, `CodeTable::sites`, indexed by the function's number (top-level code has one), a `Vec<OnceCell<Box<[RefCell<CachedSite>]>>>` that grows with the facts. `site(function, ip)` is one bounds check, the test of the one `OnceCell` main had, and the index: no walk from the function to its file (main read the function's file's array of functions, then the function's cells). The loop's helpers (`fast_properties`, `fast_static_properties`, `fast_named_calls`, `fast_builtin_guards`, `frame_view`, `pending`) call `self.code.site(function, ip)` instead of `self.code.cache(function).site(self.code.facts(function), ip)`.

The class checks are an array per file that a fork starts with empty (length 0, nothing allocated). The loop reads a cell exactly as before (one bounds check, no test for "not made yet"); the slow path of a typed parameter or return value (`check_window_argument`, `check_return_value`) makes the array when the file has none and the value was not accepted (`CodeTable::make_class_types`), and takes the snapshot's answers into it.

Nothing a request writes is in `base`: a request writes only into its own cells, and `base` is never changed. A cell that holds an object (a bound method, a closure) is never put in `base`; the fork copies none and makes it again. The snapshot's classes are shared by every fork, so the class addresses, class numbers and function numbers in `base` mean the same in a fork as they did in the boot.

**What a fork still copies**: the function facts (a `Vec<FunctionFacts>`), the file slots, the loaded-file records. The facts were shared for a while in this change (`Rc<[FunctionFacts]>`, copied by the request that changes it); see "Turned down".

`trait_method_names` is a sorted flat array searched by method number (`Rc<Vec<(u32, Rc<str>)>>`) instead of a `HashMap<FunctionIndex, String>`: it is read only to name a function in an error or a trace, and a fork shares it until it composes a trait method.

The fork's leases are as before: it keeps the snapshot's alive.

## Why

Measured on 2026-11-21 on a booted Laravel application, 80 percent of what one fork allocated (2.79 MB) was the code table's own data and 2.4M to 3.2M machine instructions went to copying it and freeing the copy. On the synthetic application of `crates/runtime/examples/fork_cost.rs` (41 files, 3,700 functions, 328 classes, 200 objects), against main `b02428a4` (the fork columns against `c4750d18`, which differs in nothing a fork touches):

| | main | this change |
| --- | ---: | ---: |
| bytes allocated per fork | 2,631,770 | 614,850 |
| allocations per fork | 4,628 | 824 |
| instructions per fork | 4,027,056 | 1,206,237 |
| time per fork (10,000 forks) | 500 to 510 µs | 184 to 186 µs |
| request that runs 4 of the 41 files, instructions | 6,151,373 | 3,452,395 |
| the same, and loads and declares 70 classes the boot did not load | 10,326,230 | 7,635,648 |
| the same, after the worker booted again with those 70 classes declared (`--settle`) | 8.12M | 5.20M |

The request gate (`~/.phera-review/bin/gate.sh`, plain Laravel 13, 12 pairs, millions of instructions per request, main `b02428a4` built here against this head): `/api/hello` 27.062 to 25.439 (-5.99 percent), `/api/posts?page=2` 111.972 to 110.347 (-1.45 percent). The gate's 200 requests include the warm-up boot of the worker (#185), so they average requests of a worker that has not settled with those of one that has.

Bytes a fork allocated by structure (`PHERA_FORK_FOOTPRINT=1`, debug build, the same application, before the facts went back to a `Vec`: add 3,700 x 88 bytes for the facts and 59,200 for the empty site table):

| structure | was | is |
| --- | ---: | ---: |
| site cells | 1,064,640 | 0 |
| per-function site boxes | 69,792 | 59,200 (one empty box per function, in one allocation) |
| per-type class checks | 313,632 | 0 |
| per-symbol function links | 65,568 | 0 |
| closure signatures | 384,096 | 0 |
| function facts | 412,016 | 412,016 (copied again, see below) |
| loaded-file records | 11,976 | 12,792 |

What is left is the facts (about a quarter of the fork's instructions and most of its time), PHP values (objects, arrays, reference cells) and the extension state map.

## Turned down

The three ways to treat the caches, measured on the synthetic application (instructions):

| | fork alone | request, 4 files | request, 4 files and 70 classes |
| --- | ---: | ---: | ---: |
| copy the snapshot's cells into every fork (main) | 4,026,173 | 6,157,127 | 10,149,515 |
| every fork starts empty | 988,587 | 4,553,384 | 9,608,077 |
| every fork starts empty and takes the snapshot's cells at first use (this) | 892,160 (with the facts shared) | 3,315,226 | 7,812,851 |

- **Starting every fork with empty caches**: the fork is as cheap, but every site, link and class check the request reaches is resolved again (a name lookup, a property resolution, a class derivation): 1.2M instructions more per request for 4 files than taking what the snapshot knows.
- **Sharing the snapshot's cells for reading and copying a file's cells at the first write**: the writes are made through `&UnitCache` by interior mutability in about ten places, some in the interpreter loop (`calls/fast_calls.rs`), so a first write cannot be intercepted without a test on every write, and a hit would need to ask which table to read. Filling the fork's own cell from the snapshot's on the miss, which exists already, has no test on a hit.
- **The function facts shared as an `Rc<[FunctionFacts]>`** (a first version of this change): the fork cost 0.89M instructions and 74 µs, because it copies nothing, and a request that loaded a file copied the table (0.27M instructions on `/api/hello`, 1.4M on `/api/posts`). But every read of the facts got dearer by about one instruction in the interpreter loop's code: on the Mac the lowest of five runs of `bench-properties` was +0.8 percent, `bench-hierarchy` +0.86, `bench-typed-objects` +0.9, and a Symfony request +0.2 to +0.4 percent, more than the fork saved. Here, the same head with only the facts back as a `Vec` (and the site boxes as they were): hierarchy -0.21 percent, typed-objects -0.12, calls -0.11 against +0.56, +0.63, +0.20 with the `Rc<[T]>`. Why one `lea` costs that much is not known (the disassembly of `CodeTable::loaded` is the same length, the displacement folds into the load); it cannot be held as a pointer and a length in the code table without `unsafe`. The cost of going back: the fork copies the facts again, 0.31M instructions and about 110 µs on this application. **Not built, and the way to get both**: a worker keeps the tables of finished requests (a pool per snapshot), takes one for a new fork instead of copying, and puts it back with what the request wrote undone from a log of writes (a few entries); the read is a `Vec`'s, the fork copies nothing in the steady state. It is a change to how a request ends, and a wrong undo is an isolation bug, so it wants a decision of its own.
- **Site boxes made per file and lazily** (a `OnceCell<Box<[Sites]>>` in each file's cache, the second version): a test on every `site()` and a walk from the function to its file; +0.3 percent on `bench-properties`, +0.15 on typed-objects and function-objects on the Mac. The flat table in the code table has main's one test and no walk; it is faster than main (properties -1.44 percent here).
- **A shared base plus the request's additions for the facts** (a fork reads `base[i]` below the base's length and its own vector above): one comparison on every read of the facts, and the decision of 2026-11-20 measured 2 to 4.5 percent for a split table.
- **Class checks made with the file (no test on a read)**: an array per file in every fork, 313,632 bytes and an allocation per file: 0.4M instructions per Laravel request. A lazy `OnceCell` of the array put a null test on every class check (`hierarchy` +1.03 percent); making the array in slow path under `&mut CodeTable` (an empty array the loop reads like a full one) removed the test.

## What it costs, and what is not covered

- Benchmarks (`benchmarks/bench-*.php`, instructions, median of 5 runs of this head against the median of 5 of main `b02428a4`, x86-64): faster than main: properties -1.44 percent, function-objects -1.02, methods -0.86, object-sort -0.83, objects -0.80, typed-objects -0.70, calls -0.62, hierarchy -0.47, framework-patterns -0.46, exceptions -0.20, closures -0.18, generators -0.14, natives -0.18, spl -0.11. Slower: arithmetic +0.97, function-loop +0.98, fib +0.29, array +0.30, function-array +0.18, reflection +1.26 (it was +0.07 and +0.10 on other days: the benchmark has two modes, 4,863 and 4,924 million). The five slower ones that do not move between runs are in the loop's own code, not in what this change adds: `bench-arithmetic` has no call, no property and no class, and its +15.0M (3 instructions per iteration, a different spill of the instruction pointer in `Machine::run`) is the same in the first version of this change (facts shared, site boxes per file), in the second, in this one, in a variant with the class checks, the closure signatures, the native links and the function links as main had them, and with `CodeTable::site` forced inline or out of line. Main `c4750d18` ran it in 1,559M and main `b02428a4` in 1,538M, so a reshuffle of `run`'s registers by an unrelated change moves it by 1.3 percent either way; nothing in this change is executed by that loop.
- A request that loads a file the snapshot does not have writes its facts into its own table: nothing is copied for that. A settled worker (#185) declares the classes in its snapshot; the files that return data (`Translation/lang/en/pagination.php` on `/api/posts`) are still loaded by each request: `CodeTable::load` makes the file's names, constants, cells and facts every time.
- Tests: `a_forks_code_table_and_caches_are_its_own_when_it_changes_them` (`crates/runtime/tests/snapshots.rs`) runs forks one after the other and at the same time, each changing something another fork and the snapshot must not see: a class declared under a name the other requests declare differently (site cells, method-site records, class checks and property entries filled for another class), a file included with a function and a class that composes a trait method (facts, slots, trait method names), a function symbol the boot left unlinked that one request links and another does not, a static property written, a closure bound to another class. `forks_of_a_replaced_snapshot_run_on_after_the_new_one_loads_and_declares` boots a worker three times with forks of the older snapshots still alive (one run before and one after the new snapshot's fork loads and declares). Without a test of their own: the native call links.
- `cargo test --workspace --no-fail-fast`: 924 passed and the same 16 failures as `~/.phera-review/baseline-78637583/tests.outcomes` on this machine (PHP oracle and database tests that need the validation setup); phpt 6,943 passed, 0 failed.

## Later: files that settled requests include, in the snapshot's code table

A request that includes a file the snapshot lacks runs `CodeTable::load`: it makes the file's names and constants as PHP values, the unit's cache, the facts of every function and the site cells' boxes, and numbers the functions through the worker. For a file of data that is most of what the include costs; the top-level code itself is a few instructions.

What it takes for the snapshot to have the file already: the warm-up (#185) records the paths of the files that forked requests include, as it records the classes they autoload, and the re-boot, before `Phera\snapshot()` keeps the state, resolves each path through the worker's code cache and calls `CodeTable::load` on its compiled unit without running its top-level code. The units then hold the file's code, facts and slots in the snapshot, and an include in a fork finds it (`find` by the unit's process number, as for any loaded file), so `load` returns at once and only the top-level code runs. What must hold: the revision the request includes is the revision loaded (the request's include cache validates by the same policy; a changed file gives another `Arc<CompiledUnit>`, `find` misses, and the request loads it as it does now), the snapshot's list of files to check for changes includes these paths (a changed data file boots again, as a changed class file does), and nothing of the file is executed in the boot, so a file that has side effects when it runs runs in each request as it does now. The per-request state of an include (`include_once`'s list of files already included) stays the request's.
