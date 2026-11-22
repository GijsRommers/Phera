# Classes that requests autoload are declared in the snapshot

- Date: 2026-11-21
- Area: Server and requests
- Decided by: Mistral Large 4 (implementation), reviewed by Fable

## Decision

A `phera serve` worker keeps, per front-controller script and only in its own
memory, the list of classes its forked requests autoloaded. When the list has
not grown for 64 requests (`WARMUP_SETTLE_REQUESTS`) and fewer than 3
warm-up boots have run (`WARMUP_MAX_BOOTS`), the worker boots the script
again in the background, exactly like a changed file does. Just before
`Phera\snapshot()` keeps the boot's state, the boot calls the registered
autoloaders for each class in the list, in the order the requests first
loaded them, so the files' top-level code runs as it would have. A class
whose load fails or throws is dropped from the list and loads in the request
as before; the boot never fails because of the warm-up. From then on, forked
requests find the classes declared and run no autoloader for them.
(`crates/runtime/src/snapshots.rs`, `crates/runtime/src/lib.rs`,
`crates/vm/src/snapshot/intrinsic.rs`, `crates/vm/src/autoload.rs`)

This is on by default; `phera serve --no-class-warmup` turns it off.
Snapshot and fork stays on either way. The list is rebuilt from the worker's
own requests: nothing is written into the project or read from a previous
server run. A changed file still boots again and the list survives that
boot. Requests in flight during a warm-up boot finish on the old snapshot;
no request waits for the boot. Only classes an autoloader call produced are
listed, so anonymous classes, classes declared conditionally inside a
function the request then calls, and `eval` classes are never in it.

A warm-up boot starts only when the list has grown since the last boot that
used it (the worker remembers the length that boot declared) and has then
been stable for 64 requests; a settled worker whose requests load nothing
new therefore never boots again, and at most 3 warm-up boots run per script
per worker (`note_forked_request` in `crates/runtime/src/snapshots.rs`). A
class the boot cannot load any more (its file was deleted or renamed, or
the load throws) is removed from the list, so a later boot does not try it
again; it loads in the request as before.

The one way a program can tell the difference from PHP — the same stated
difference `opcache.preload` has: a warmed class exists before the request
starts, so `class_exists($name, false)` is true and the autoloader is not
called for it in the request. An autoloader with a side effect (logging,
counting) therefore sees one call per warmed class in the warm-up boot and
no calls in requests after it settles, where PHP calls it once in the first
request that needs the class; with `--no-class-warmup` Phera calls it in
every request, as before.

## Why

Every request of a booted-but-not-preloaded application re-pays the autoload
of the classes it uses: the autoloader's PHP code, the file checks, the
include, the declare and the link. Measured on main `8ec21a91` (counting
build + samply; method below), one `/api/hello` request autoloads 70 classes
and declares 94, and the whole load costs 3.1–4.1M of its 36.2M instructions
(9–11%); `/api/posts?page=2` pays 9.3–10.0M of 136.0M (131 loads) and the
Symfony route 44–56M of 767.9M (337 loads). `--preload` does not remove it,
because these loads happen during request handling, not at boot (32.08M →
31.99M at c=1 on hello). Declaring them once in the snapshot removes that
cost from every settled request. Measured with the warm-up on (kernel
counters, 200 measured requests after 150 warm-up requests per run, one
worker, alternating base first):

| route | pairs | main `78637583` | with warm-up | difference | responses |
|---|---:|---:|---:|---:|---|
| `/api/hello` | 12 | 32.39M instr/req | 27.77M | −14.3% | byte-identical |
| `/api/posts?page=2` | 12 | 127.29M | 116.32M | −8.6% | byte-identical (modulo the listen port in pagination URLs) |
| `/api/db-yardstick` (Symfony) | 4 | 710.6M | 674.1M | −5.1% | byte-identical at equal request parity; the `digest` field identical in every run |

The worker settles after 64 requests without list growth; on both Laravel
routes the new snapshot was serving from request ~76 on (windowed
measurement: the window covering requests 51–75 carries the concurrent boot,
requests 76+ run at the settled cost).

What one autoloaded class costs today (main `8ec21a91`, `/api/hello`: 70
autoloader runs, 94 `declare_class` per request). Autoloader PHP cost is its
fnop-op count times the route's measured instructions per op (277 for hello);
the VM side is the profile's class-loading row; the file-stats row bounds the
stat/access calls (their individual count was not measured):

| component of one autoloaded class | per request | per class | instructions per class |
|---|---:|---:|---:|
| `AliasLoader::load` | 2,030 fnop ops | 29 ops | ~8.0K |
| `ClassLoader::findFile` (excl. syscalls) | 980 fnop ops | 14 ops | ~3.9K |
| `ClassLoader::loadClass` | 910 fnop ops | 13 ops | ~3.6K |
| `includeFile` | 420 fnop ops | 6 ops | ~1.7K |
| included file's top-level code | 266 fnop ops | 3.8 ops | ~1.1K |
| **autoloader PHP, subtotal** | 4,606 fnop ops | 66 ops | **~18.2K** (1.28M/req) |
| VM: autoload lookup, `Include` ×70, `DeclareClass` ×94, code-table unit load, compile, link | 1.81M instr/req | | **~25.9K per load** (19.3K per declare) |
| file stats (stat/access/getattrlist) | 0.99M instr/req | | **≤14.1K per load** (upper bound) |
| **total** | **3.1–4.1M instr/req** | | **≈ 44–59K per autoloaded class** |

Per class on the other routes: posts 9.3–10.0M/req over 131 loads ≈ 71–76K;
Symfony 44–56M/req over 337 loads ≈ 131–166K. PHP 8.5 with opcache pays about
0.2K instructions per warm class (26.3K instructions in the warm load window
of the posts profile over 131 loads) and, with `opcache.validate_timestamps=0`,
stats nothing per request (its whole file-system row is 0.64M/req on hello,
against Phera's 0.99M). Loading itself is the next cost to attack; this change
only removes the repetition.

The settle rule: a warm-up boot starts when the list has grown since the
last boot that used it and then saw 64 requests without growth — 1–6 s at
the gate's 40–200 requests per second, and a route hit once an hour still
lands in the list at the next settle; the cap of 3 warm-up boots per script
per worker bounds a site whose routes keep growing forever. The list is
data of the worker, not a cache: the only invalidation is the existing "a
file changed, boot again", which reloads the classes from the new files.
