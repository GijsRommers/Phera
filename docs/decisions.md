# Decisions

One entry per decision: when, what, why, and what was rejected. Newest
last. Design detail lives in the module documentation each entry names.

## 2026-11-19 — Destructors: hand the object over in `Drop`

**Decision.** An object whose class declares `__destruct` is created with
its destruction `Due`. When its last `Rc` goes, `Drop for Object` moves its
contents (class, properties, native state, attachments, its hold on its id)
into a new `Rc<Object>` marked `Done` and queues that on the request's id
pool. The interpreter runs the queue at instruction boundaries, before the
next instruction. (`crates/value/src/destruction.rs`)

**Why.** PHP runs a destructor at the moment the count reaches zero. A Rust
`Drop` cannot run PHP code (no interpreter, no `await`), and by then the
`Rc` is gone, so the object cannot be `$this`. Rebuilding it from moved
fields needs no `unsafe`, loses no destructor and runs none twice.

**Rejected.** Reviving the dropped allocation (needs `unsafe`); scanning for
dead objects after each statement (cost in proportion to the heap, and the
moment is wrong); a tracing collector instead of reference counts (changes
every value in the VM, and PHP's moment is reference counting's moment).

## 2026-11-19 — A plain and a guarded copy of the interpreter loop

**Decision.** `Machine::run` is generic over `const GUARDED: bool`. The
batch runs the plain copy while no live object has a destructor due (a
count on the id pool), otherwise the guarded copy, kept out of line. The
plain copy is `main`'s loop; only the guarded one checks releases in stores,
returns and switches. Creating an object with a destructor leaves the plain
loop. (`crates/vm/src/calls/execution_loop.rs`, `crates/vm/src/fast.rs`)

**Why.** Checking for due destructors in every store cost 2 to 8 percent on
the benchmarks, for programs that mostly have no destructors at all. Under
fat LTO the hot loop's code generation is sensitive to any added branch;
two copies keep the common one unchanged.

**Rejected.** One loop with a cheap flag test per store (measured 1 to 3
percent); checking only at statement ends (PHP's moment is the release,
inside expressions too).

## 2026-11-19 — Object identity is the original allocation's address

**Decision.** `WeakObject` (weak references, WeakMap keys, a fiber's handle
on itself) recognises an object by the address of the allocation it was
created in (`Object::identity`). The object that waits for its destructor
carries the identity of the one it took over from, recorded with its id in
the pool's incarnations. The id goes back to the pool through `PoolHold`,
the object's last field, so inner objects' ids are freed before the outer
one's, as in PHP. (`crates/value/src/weak_object.rs`,
`crates/value/src/object.rs`)

**Why.** A weak handle must still see the object while its destructor runs
(the `Rc` is new then), and must not mistake a later object with the same
id for it. A `Weak` keeps its allocation reserved, so the address is unique
for as long as the handle exists. This keeps `Object` at 128 bytes, the
size of `main`'s: at 144 bytes the allocator took its slow path on most
allocations (about 200 more instructions per object).

**Rejected.** A per-object serial number (the first version: 16 more bytes
per object, measured above); keeping the id lease as its own field holding
the id (8 more bytes); giving the id back first in `Drop` (changes the
order in which PHP reuses ids, which `spl_object_id` and `var_dump` show).

## 2026-11-19 — The cycle collector

**Decision.** Trial deletion over the request's live objects on `Rc`
counts, iterative, with flat edges (`crates/value/src/cycles.rs`). Objects
that hold no heap values and are not attached to anything (leaves) are
skipped and counted at the end. A run is asked for after a number of
object allocations: PHP's adaptive threshold (10,000, growing when a run
frees little), and never before four times as many allocations as there
are live objects. Destructors of garbage run first, then the search is
repeated, then what is still garbage is broken apart. WeakMap values live on
their key objects, so freeing a key frees its value at PHP's moment.
(`crates/vm/src/cycles.rs`, `crates/value/src/attachments.rs`)

**Why.** PHP frees cycles with its collector and scripts depend on it (a
million two-object cycles took 700 MB on `main`, 18 MB here, 28 MB in PHP).
Counting allocations instead of PHP's root buffer needs no work on every
decrement. The four-times rule keeps a large live heap without cycles at a
constant cost per allocation.

**Rejected.** A root buffer filled on every decrement, as PHP does (a cost
in the hot paths of every release); walking only at the end of the request
(memory grows without bound in long scripts and workers).

## 2026-11-19 — Temporaries are released by the compiler

**Decision.** The compiler clears a call's operands right after the call,
and the temporaries of `if`/`while`/`do`/`for` conditions and `foreach`
subjects on each way out, on top of the statement-end clears of #39. A
register that provably holds a scalar, or a copy of a variable the
expression does not assign, is not cleared; a condition starts with clean
registers. (`crates/compiler/src/expressions/operand_release.rs`)

**Why.** PHP frees a call's temporary arguments when the call returns, so a
destructor runs before the result is used (Symfony's service configurators
register themselves in `__destruct` at the end of a fluent chain). Clearing
copies of variables cost 5.9 percent on bench-object-sort and freed nothing.

**Rejected.** Reference-counting temporaries at run time (cost on every
instruction); clearing at statement ends only (too late for conditions,
which enclose their whole body).

## 2026-11-19 — Fatal errors and timeouts end the request in one way

**Decision.** After a fatal error, including the execution time limit, every
object alive is marked as destructed; shutdown functions run; objects they
create get destructors in the normal end-of-request phases. A suspended
fiber is destroyed (unwound) like an object with a destructor: when its last
reference goes, or in those phases, never after a fatal error.
(`crates/vm/src/request_end.rs`, `crates/vm/src/fibers/lifecycle.rs`)

**Why.** That is PHP's behaviour, and one path serves every fatal error.
The time limit's own handling of objects created after the timeout (from
#86) did the same job and was removed.

**Rejected.** Running no destructor at all after a fatal error (PHP does run
those of objects the shutdown functions create); closing all suspended
fibers before the destructor phases (PHP destroys them in the phases'
order).
