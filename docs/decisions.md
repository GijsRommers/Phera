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

## 2026-11-19 — What a request may change about a class lives outside the class

**Decision.** A linked class holds no state a script can change. Static
properties, the values of class constants evaluated on first read, and the
flags that say whether either was initialised live in tables of the request
(`ClassStates`), reached through slot numbers the class keeps; a child that
inherits a static shares its parent's slot. The method tables used by fast
calls move from the class into the request's class table, because they are
keyed by the request's own name numbers. (`crates/vm/src/classes/class_state.rs`,
`crates/vm/src/classes/class_table.rs`)

**Why.** A snapshot and every request forked from it share one `Rc<Class>`
per class: copying 500 classes per request would cost what the snapshot is
meant to save, and a shared class must not carry one request's static
values into another. It is also the first half of PHP's inheritance cache.

**Rejected.** Copying classes in every fork (cost per request, and parent
pointers would need remapping); keeping method tables in the class (a table
built in one fork after it numbered a new name could map that number to the
wrong method in another fork).

## 2026-11-19 — Classic requests do not share linked classes yet

**Decision.** Step 1 of architecture 5.4 is built as far as forks need it
(immutable classes, request state outside them). Sharing the linked classes
between classic requests of a worker, PHP's inheritance cache, is not built
in this change.

**Why.** Function and class numbers are given per request in load order, so
a class linked in one request refers to code by numbers another request
does not share. Making them worker-wide changes `CodeTable::load`, which the
include-caching work is changing at the same time, and saves 45 to 60
million instructions per classic Laravel request, against 210 to 285 million
for a fork, which already starts with every class linked. An application
with a snapshot gets no benefit from it.

**Rejected.** Doing it first, as 5.4 orders: the larger and more conflicting
change for the smaller gain.

## 2026-11-19 — One primitive: a fork of the whole VM

**Decision.** `Phera\snapshot()` is a VM intrinsic. In a boot run it copies
the VM as it is at the call (`Vm::fork_vm`), keeps the copy as the snapshot,
and stops the run with an unwind that no `catch` or `finally` sees. Every
request copies the snapshot the same way and resumes it: the call's result
register gets `true`, and the interpreter continues from the frames as they
were. (`crates/vm/src/snapshot/`)

**Why.** Frames live on the heap and the loop publishes its position before
a native call, so the VM struct is the whole state: no suspension machinery
is needed, and the same copy serves the snapshot and the forks. Copying at
the call also tests at the call that the state can be kept, so a refusal is
an exception there, with a path to the object.

**Rejected.** Suspending the boot run like a fiber and copying later (the
Rust future would hold the VM borrowed); running the boot again per request
from a cached prefix (does not save the boot).

## 2026-11-19 — Boot at server start, without a request

**Decision.** For a script known to snapshot (an adapter applies, or an
earlier start found the call), each worker runs it before accepting
connections, with no request: `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES` and
`$_REQUEST` empty; `$_SERVER` holds the environment plus `SCRIPT_FILENAME`,
`SCRIPT_NAME`, `PHP_SELF`, `DOCUMENT_ROOT`, `SERVER_SOFTWARE`, `REQUEST_TIME`
and `REQUEST_TIME_FLOAT` (what PHP's command line also sets without a
request, minus `argv`, which a web request has not either); `php://input` is
empty; output is discarded. Every request, the first included, is a fork.
The first start of a plain script cannot know: its first request runs
classically, notices the call, remembers the script in
`$XDG_CACHE_HOME/phera/snapshots/` (else `~/.cache/phera/snapshots/`) and
boots it in the background. A boot that ends without the call makes the
script classic until one of its files changes; a boot that fails is logged
with the worker's number and retried when a request comes after 2 seconds.
(`crates/runtime/src/boot.rs`, `crates/runtime/src/snapshots.rs`)

**Why.** The reviewer's change: no user pays for the boot, compiling the
framework happens before the first connection, and no request's values can
leak into the state every request starts from, because there is none.

**Rejected.** Snapshotting during the first real request (its superglobals
leak into the snapshot); booting every front controller at start (a plain
script has side effects it should not run without a request).

## 2026-11-19 — The copy: objects once, arrays shared unless they hold identity

**Decision.** Every object and reference cell reachable from the VM is
copied once (same id, same class), so sharing and cycles stay; an object is
made empty and queued, then filled, so no copy recurses over the data. An
array is shared unless it holds an object, a reference or a resource
somewhere inside; then a clone of it is rebuilt (keeping its internal
pointer and next free key). Native state goes through a fork hook per class
(`ClassHooks::fork`), open resources through a hook per kind
(`Registry::fork_resource`); a class or resource without one refuses.
(`crates/value/src/fork/`)

**Why.** That is architecture 5.4's contract. Sharing scalar arrays keeps
the copy proportional to the objects, which is what a booted framework
mostly holds.

**Rejected.** Copy on first write (step 4 of 5.4): a change to the value
representation, to be proposed only if the copy shows in profiles.

## 2026-11-19 — Every extension says how a fork gets its request state

**Decision.** `RequestState` remembers each slot's type name, and a fork
copies each slot with the policy its extension registered
(`Registry::fork_state`): `fresh_state` (the fork starts without it, as a
new request does: response headers, uploads, json, pcre, libxml and openssl
errors, random number seeds, serializer and reflection caches),
`copy_state` (a setting the boot made: timezone, locale, working directory,
mbstring and iconv settings, `openlog()`), or a function that copies PHP
values inside (the `WeakReference` registry, libxml's entity loader, a
session save handler, the stream table). A type without a policy refuses
the snapshot by name. A session started before the call is refused.

**Why.** The VM cannot know what extensions keep; a default of "start
fresh" or "copy" would be silently wrong for some type. Refusing by name
makes a forgotten type visible the first time a snapshot holds it.

**Rejected.** A trait bound on every state type (changes every call site of
`get_or_default`).

## 2026-11-19 — What a fork resets, and what is not state

**Decision.** A fork has its own superglobals and request time, an empty
header list and response status, empty output buffers (the buffers opened
before the call stay open, without their contents: output written before
the snapshot is never sent), no last error, the time limit in force at the
call with no time used, and fresh seeds for `rand()`, `mt_rand()` and
`random_int()`. `microtime()`, `hrtime()`, `uniqid()` and `getmypid()` read
the clock and the process, not request state: nothing to reset; all forks
of a worker report its process id, as all requests of a PHP-FPM worker do.
Object ids continue from the snapshot's: the first object a request makes
gets the same id in every fork, as in PHP-FPM where every request runs the
same boot. (`crates/vm/src/snapshot/copy.rs`)

## 2026-11-19 — Destructors in a snapshot

**Decision.** A copied object whose class has `__destruct` is destroyed at
the end of its fork's request, as PHP-FPM destroys the boot's objects at the
end of every request. The snapshot's own objects are never destroyed: not
while the worker lives, not when the snapshot is replaced, and not at worker
shutdown. (`crates/value/src/fork/objects.rs`)

**Why.** The snapshot is never run, only copied; its destructors would run
side effects (closing connections, flushing logs) that no request asked for.
Each request already ran them on its copies.

## 2026-11-19 — A changed file boots again in the background

**Decision.** A snapshot remembers the files its boot included and their
modification times. They are checked as opcache checks includes
(`opcache.validate_timestamps`, at most every `opcache.revalidate_freq`
seconds, 2 by default; 0 checks on every request). A change starts a new
boot on that worker in the background; requests keep forking the old
snapshot until the new one replaces it. (`crates/runtime/src/snapshots.rs`)

**Why.** The reviewer's rule: never block requests on a re-boot. The same
two settings as includes, so one configuration covers both.

## 2026-11-20 — No warning for request fields read before the snapshot

**Decision.** Not built. Because the boot runs without a request, a boot
that reads `$_GET`, `$_COOKIE`, a `REQUEST_*`, `HTTP_*`, `REMOTE_*` or
`QUERY_STRING` key of `$_SERVER`, or `php://input` gets nothing: an empty
array, a missing key (PHP's own "Undefined array key" warning for a direct
read), an empty body. No request's value can reach the snapshot.

**Why.** Seeing the silent reads (`$_SERVER['HTTP_HOST'] ?? 'localhost'`)
needs a test on every array read, or superglobals that are objects during
the boot (`is_array()` would then differ between the boot and a request).
The first costs every script; the second changes what the boot computes.
`phera serve` has no `--dev` mode on this base to hang it on either.

**Rejected.** A flag on the superglobal arrays checked by every read.
