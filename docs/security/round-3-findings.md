# Security findings, round 3 — sessions, uploads, unserialize, static files, limits

Third round of the authorized defensive testing. Tested commit:
`54c9ec2e48744c6ea31d9ecb3f1a8a88139acbfc` (`origin/integration/batch-6`, the
branch that adds sessions, backtraces and the type checker to `main`; checked
out as `security/round-3`), release build `cargo build --release -p phera-cli`
(binary `target/release/phera`, `target/` stayed at 0.74 GB, well under the
8 GB clean threshold). Date: 2026-11-19.

Method as in rounds 1 and 2: raw-socket Python (stdlib only) for the wire
level, PHP scripts run through both `phera` and the HTTP server for engine
semantics, every finding confirmed **three times against freshly started
servers** (`--workers 1`, `127.0.0.1`, ports 18400-18499 only, `nice -n 15`),
compared with real PHP 8.5.12 (`/opt/homebrew/bin/php -n` and
`php -n -S 127.0.0.1:PORT`) wherever PHP's behaviour is the yardstick. The
project's own test containers were not needed; the PostgreSQL on 5432 was not
touched; nothing outside this worktree and the system temporary directory was
written.

## Summary

| # | Name | Severity | One line |
| --- | --- | --- | --- |
| R3-1 | Multipart boundary scan is O(body × boundary) on a thread the request timeout cannot cancel | high | A `Content-Type` with a ~200-500 KB `boundary` plus an 8 MiB body makes the client get its 504 at the timeout while the server keeps burning a core for minutes; a few such requests pin every core. |
| R3-2 | Uploaded file's temp copy is not deleted when the request is aborted | medium | A request that uploads and then outlives `--request-timeout` (504) leaves its `phpXXXXXX` file (0600) in the temp dir forever; repeatable, fills the disk. |
| R3-3 | `allowed_classes` is not honoured inside SPL containers' own unserialize payloads | medium | `unserialize($d, ['allowed_classes'=>['ArrayObject']])` instantiates arbitrary nested `O:` classes (PHP makes them `__PHP_Incomplete_Class`) — the object-injection primitive `allowed_classes` exists to prevent. |
| R3-4 | A request body trickled at < 1 byte / 30 s is held forever (round-2 F4 residual) | medium | `--request-timeout` does not cover body reads (they finish before the handler task exists); 10,000 trickling connections hold every worker slot indefinitely. |
| R3-5 | `phera new` writes through an existing symbolic link as its target | low | `phera new shop` with `shop -> empty-dir` writes the project into the linked directory (Composer argument injection is not reachable: the CLI rejects leading `-`). |

Findings from earlier rounds that changed status are in the status table at the
bottom: F1, F2, F5, F6, F8, F9 fixed and verified; F3, F4 (residual = R3-4), F7
still present; the engine's identity-cycle and selfref-cursor crashes are
fixed, the long-chain crash and the quadratic class linking are still present.

---

## R3-1 — multipart boundary scan: O(body × boundary), not cancellable by the request timeout (high)

- **Reproducer:** `tests/security/http/repro_r3_multipart_boundary_cpu.py`
  (starts its own fresh server with `--request-timeout 8`, sends one POST).
- **Request:** `Content-Type: multipart/form-data; boundary=<200,000 bytes of
  '-' ending in '!'>` with an exactly 8 MiB body of `-` bytes (the server's own
  body cap). hyper accepts header values up to between 500 KB and 1 MB, so
  the boundary can be ~0.5-1 MB.
- **Observed (3/3 fresh servers):** the client gets `504 the request took
  too long` at exactly 8.0 s; the server's cumulative CPU time then keeps
  growing after the response — measured +17.0 s, +23.2 s, +24.5 s of CPU over
  the 30 s after the client was answered (total 22.8-36.7 s and still climbing
  when the reproducer stops watching). Extrapolated with a 500 KB boundary the
  scan is ~4.2e12 byte comparisons, i.e. minutes of one full core **per
  request**, and `tokio`'s blocking pool runs up to 512 of them, so a handful
  of concurrent requests pins every core of the machine. Other requests are
  still scheduled (the worker thread itself is free), but the machine's CPU is
  consumed and, under `nice 15`, everything else on the box slows with it.
- **What PHP does:** php-src's rfc1867 reader scans with a memchr-style
  search; the same request costs O(body) there.
- **Why (code):** `crates/runtime/src/http/multipart.rs` `find()` is
  `haystack.windows(needle.len()).position(...)` — O(n·m) with the attacker-
  chosen m. The parse runs in `tokio::task::spawn_blocking`
  (`crates/runtime/src/lib.rs`, `receive_form`), and `answer()` in
  `crates/server/src/connection.rs` aborts only the *handler* task at the
  request timeout; a `spawn_blocking` task cannot be aborted, so the scan runs
  to completion after the 504. Measured scaling: boundary 500 → 0.13 s,
  2,000 → 1.08 s, 8,000 → 2.89 s, 32,000 → >10 s for the same 8 MiB body —
  linear in the boundary length.
- **Suggested fix:** find the boundary with `memchr::memmem::find` (memchr is
  already a dependency; it is SIMD-linear), and additionally cap the accepted
  boundary length (e.g. 4 KB, far above any real form) at
  `multipart::boundary()`. Belt and braces: give `read_body`/the form parse a
  *total* deadline (not just per-piece), so a slow body cannot outlive the
  request either (see R3-4).
- **Severity:** high — one cheap request (8 MiB body, one big header) buys
  minutes of uncancellable CPU on a blocking thread; N concurrent requests
  (N = core count) saturate the machine for as long as the attacker keeps
  sending them. Memory stays bounded (the 8 MiB body cap), so this is pure CPU
  denial of service, repeatable indefinitely.

## R3-2 — upload temp files survive an aborted request (medium)

- **Reproducer:** `tests/security/http/repro_r3_upload_temp_leak.py` (fresh
  server with `--request-timeout 3` and a private `TMPDIR`, one multipart
  POST of a 21-byte file to a script that sleeps 30 s).
- **Request:** `POST /slow.php?s=30` with a valid `multipart/form-data` body
  (one file part).
- **Observed (3/3 fresh servers):** `504` at 3 s; afterwards exactly one new
  `phpXXXXXXXXXX` file (mode 0600, owned by the server user, containing the
  uploaded bytes) remains in the temp directory: `phpl1G18D`, `php5BsJMb`,
  `phprPskCf` on the three runs. Normal requests, fatal-error requests and
  client disconnects do delete their temp files (verified separately); only
  the abort path leaks.
- **What PHP does:** PHP deletes the upload temp file in
  `php_request_shutdown` / when the request is torn down, including on
  `max_execution_time` expiry; nothing is left behind.
- **Why (code):** `crates/runtime/src/lib.rs` `handle_web_request` deletes the
  unclaimed files only in the code *after* `vm.run(unit).await` returns. When
  `answer()` (`crates/server/src/connection.rs:69-86`) calls `task.abort()` at
  the request timeout, that future is dropped mid-`run`, the cleanup never
  executes, and `UploadedFiles` (request state, `crates/extensions/http/src/
  uploads.rs`) is dropped without a `Drop` impl that unlinks. The same leak
  happens if the handler task panics (`finished.unwrap_or_else(...)` → 500).
- **Suggested fix:** give `UploadedFiles` a `Drop` impl that removes the files
  it still holds (the request owns it exclusively), or register the paths for
  deletion with the request's cancellation guard; additionally sweep stale
  `php*` files in the temp dir at worker start.
- **Severity:** medium — one leaked file per aborted upload, 0600 so only the
  server user can read it, but an attacker who can make requests run long
  (any slow endpoint) fills the temporary filesystem with request-sized
  files; on a container with a small `/tmp` that is disk exhaustion of the
  host, and the files also retain whatever the client uploaded.

## R3-3 — `allowed_classes` bypass inside SPL containers' unserialize (medium)

- **Reproducer:** `tests/security/repro_r3_allowed_classes_spl.php`
  (`phera tests/security/repro_r3_allowed_classes_spl.php`, exit 1 =
  vulnerable; run with `/opt/homebrew/bin/php -n` for the reference).
- **Script:** `unserialize('C:11:"ArrayObject":43:{x:i:0;O:4:"Evil":1:{s:1:"x";i:9;};m:a:0:{}}',
  ['allowed_classes' => ['ArrayObject']])` with a plain `class Evil {}`.
- **Observed (3/3 runs):** Phera returns an `ArrayObject` whose storage is a
  **real `object(Evil)#` with `x=9`** (`var_dump` shows `object(Evil)#2`).
  Real PHP 8.5.12 returns `object(__PHP_Incomplete_Class)#2` with
  `__PHP_Incomplete_Class_Name = "Evil"` — the nested class is blocked, exit 0.
- **Why (code):** the outer `C:` object is checked against `allowed_classes`
  in `crates/extensions/serialize/src/decode/objects.rs` (`class_info` →
  `options.allows`), but the payload is then handed to
  `ArrayObject::unserialize`, whose parser (`crates/extensions/spl/src/legacy/
  read.rs`, the `O:` branch) calls `context.create_object(class, ...)` for any
  class name with no allowed_classes check. It only refuses classes with
  native layout, `WeakReference`, `Closure`, `__unserialize` or `__wakeup`
  (matching PHP's "uses custom serialization" rule).
- **Suggested fix:** thread the `Options`/`allowed` set into the SPL legacy
  reader and make its `O:` branch apply the same `allows()` check (turning the
  object into `__PHP_Incomplete_Class`), exactly as PHP's nested
  `php_var_unserialize` does.
- **Severity:** medium — an application that unserializes attacker-controlled
  data with an allow list (the standard mitigation for PHP object injection)
  still gets arbitrary non-allowed classes instantiated with attacker-chosen
  properties, as long as one allowed class is an SPL container. Gadget classes
  with `__wakeup`/`__unserialize` are still refused (both engines), so this
  needs a property/destructor-style gadget, but that is the common case.

## R3-4 — trickled request body is held forever; the request timeout does not cover body reads (medium, round-2 F4 residual)

- **Reproducer:** `tests/security/http/repro_r3_trickle_body.py` (fresh server
  with `--request-timeout 10`, one connection sending 1 byte every 5 s).
- **Request:** `POST /post.php` with `Content-Length: 1000000`, then one byte
  every 5 s.
- **Observed (3/3 fresh servers):** no response and the connection still open
  after 40 s (8 bytes sent) on a server whose request timeout is 10 s. A body
  that goes fully silent gets `408` at exactly 30.0 s and an idle keep-alive
  connection is closed at exactly 75.0 s (both verified) — so the new timers
  work — but a trickle slower than one byte per 30 s evades both, forever.
- **What PHP does:** PHP's web SAPI has no body timeout either, but it does
  bound the whole request with `max_execution_time` and a request shutdown;
  more importantly a proxy in front usually caps request duration. Phera's own
  architecture (docs/architecture.md §5.1) promises "whole request, waiting
  included: 60 s".
- **Why (code):** `read_body` (`crates/server/src/connection.rs:127-160`) is
  awaited inside `to_request`, which runs *before* the handler task that
  `answer()` wraps in `tokio::time::timeout`; the request timeout therefore
  never covers body reading. The per-piece timer is
  `timeout(body_idle_timeout, body.frame())`, reset by every frame.
- **Suggested fix:** wrap the whole `to_request` + handler (or at least
  `read_body`) in one deadline equal to `request_timeout`; answer 408/504 and
  close.
- **Severity:** medium — one connection per few bytes is held open
  indefinitely at ~no memory cost; the per-worker cap is 10,000 connections
  (verified: with 10,050 open, a new control request was not answered while
  the cap was full, server RSS only 82 MB), so one client can hold all slots
  with trickling uploads and no other client is served until it stops. The
  75 s idle timeout does not fire because these connections are never idle.

## R3-5 — `phera new` writes through an existing symbolic link (low)

- **Reproducer:** `tests/security/http/repro_r3_new_symlink.py` (uses
  `target/release/phera`, a private temp dir, `--template plain
  --no-install`).
- **Command:** with `shop -> <empty dir>` present: `phera new shop --template
  plain --no-install`.
- **Observed (3/3 runs):** exit 0 and the project files (`README.md`,
  `composer.json`, `public/`, `src/`, `.env`, `.gitignore`) land **inside the
  symlink target directory**, not at a path named `shop`.
- **What PHP/Composer do:** `composer create-project` also follows a symlinked
  target; the interesting part is that `phera new`'s own guard
  (`refuse_existing_files`, `crates/cli/src/new.rs`) reads through the symlink
  and only refuses non-empty targets, giving a false sense of "never
  overwrites anything".
- **Not exploitable via arguments (checked):** the CLI parser rejects any
  argument starting with `-` ("Unknown option: --working-dir=..."), `--` is
  not a separator, and `project_name()` validates the basename to
  `[A-Za-z0-9._-]`; a name like `x/--prefer-dist/y` reaches Composer as one
  positional path (verified with a fake `composer` on `PATH` logging its
  argv: `create-project symfony/skeleton:7.4.* x/--prefer-dist/y ...`). No
  shell is involved (`Command::args`), so no shell injection.
- **Suggested fix:** in `refuse_existing_files`, `symlink_metadata` the
  directory first and refuse (or resolve and require an empty real directory)
  when it is a symlink.
- **Severity:** low — the user names the directory, but a pre-created
  `shop -> ~/.ssh`-style symlink in a shared setup makes `phera new shop`
  drop files into the linked location.

---

## Per-area coverage: what was tried, for how long, what held

### Area 1 — file uploads / multipart (`crates/runtime/src/http/multipart.rs`, `uploads.rs`, `crates/extensions/http/src/uploads.rs`)

Tried (~1.5 h): basic upload shapes; `filename` with `../`, backslashes, NUL
(`name` is the basename after the last `/` or `\`, `full_path` keeps the raw
client string — PHP 8.1+ parity); 2 MiB exactly vs 2 MiB+1 (error 1, no file
kept), `MAX_FILE_SIZE` (error 2), cut-off body (error 3, `complete=false`);
21 files (beyond 20 silently dropped, PHP also drops), 3,000 fields (first
1,000 kept, `max_input_vars`), 5,000 parts, `a[b][c]…` 3,000 deep (dropped at
nesting 64 like PHP), 60 KB field names, duplicate names, quoted/empty/
missing boundary, boundary bytes inside content, 8 MiB body cap (8 MiB+1 →
413), duplicate `Content-Type` (parser uses the first, `$_SERVER` reports the
joined value), temp file permissions (0600, server uid, inside `$TMPDIR`),
deletion after normal/fatal/disconnect requests, `is_uploaded_file()` /
`move_uploaded_file()` on `/etc/passwd`, empty and `..`-laden `from` paths
(PathBuf equality is lexical, no canonicalization trick works), and the
long-boundary CPU case (R3-1) plus the abort leak (R3-2).
**Held:** no path traversal into the temp dir (tempfile crate names), no
upload landing outside the temp dir, no `$_FILES` confusion. **Found:** R3-1,
R3-2.

### Area 2 — sessions (`crates/extensions/session/`)

Tried (~1.5 h): every id shape that could leave the save path (`../`, `%2f`,
`/`, `\`, `.`, `;`, space, NUL, CRLF, non-ASCII, 257 chars) — all rejected by
`id::valid()` (A-Za-z0-9, `-`, `,` only, ≤256) with a warning and a fresh id;
strict mode off (attacker-chosen unknown id accepted and file created — PHP
8.5 default parity) and on (unknown id → new random id, no file created;
existing id works); `save_path` with depth `2;0600;dir` (fails like PHP, which
also does not create the subdirs); GC with `gc_maxlifetime=0, probability=1`
(kept live files, same as PHP); aborted request does not write its session
changes (file left empty, not poisoned); session lock: request B with the
same id waits for A (5.7 s measured), a different id is instant, and when A
was aborted at the timeout B acquired the lock immediately — one client can
stall only its own session, not the worker; 3,000 distinct session ids did
not grow RSS (the gate map self-cleans at HashMap resize); attacker-written
session files with hostile payloads in all three formats (`php`,
`php_serialize`, `php_binary`: announced-huge counts, 100,000-deep nesting,
`R:` loops and forward refs, objects, garbage bytes, `C:`, enums) — decoded
or rejected in ≤1 ms each, no panic, no hang, no announced-size allocation;
`__wakeup`/`__unserialize` run on `session_start()` (precedence
`__unserialize` > `__wakeup`, same as PHP); `session_decode()` with an active
session on the same hostile corpus — all `false`, fast; `session_start()`
after output → `false` with warning; cookie params with CRLF in path/domain/
SameSite → refused by the header validator, no `Set-Cookie` emitted.
**Held everything; no findings.**

### Area 3 — `unserialize()`, `session_decode()`, SPL unserializers

Tried (~2 h): a 94-case CLI battery (truncations of every prefix of object
and `S:` payloads, announced-huge counts/strings/`C:` lengths, negative and
i64-max counts, `R:`/`r:` self/forward/huge slots, `R:` in keys, weird class
names, typed/readonly property confusion — TypeError like PHP, float forms,
depth 5,000 (rejected at the 4,096 default like PHP) and 100,000 with
`max_depth=0`, hooks order and hook-on-parse-failure — byte-identical to PHP
8.5.12 on every case compared, including `allowed_classes` for nested objects,
`C:` format and enum cases), timing/memory on ~4 MB inputs (distinct int keys,
distinct string keys, duplicate keys, 500,000 `R:1`s, 4,000-deep nesting,
20,000 objects): all linear, ≤0.01 s, ≤45 MB RSS — no quadratic found.
`session_decode()` hostile corpus (see area 2). SPL: `ArrayObject` nested
payloads — **found R3-3**; `SplFileObject`/`DirectoryIterator`/`GlobIterator`
with hostile patterns ran clean.
**Found:** R3-3. Note: `php://filter` chains are not implemented (all
`file_get_contents('php://filter/...')` return false, failing closed) — no
cost, no bug.

### Area 4 — static files, ranges, precompressed siblings, compression, content types, special files

Tried (~1 h): every `Range` shape on a 20-byte and an empty file (single,
open-ended, suffix, `-0`, reversed, multi-range → whole 200, other units,
overflow digits → 200 whole, 416 with `bytes */N` past the end); `If-None-
Match`/`If-Modified-Since` shapes; precompressed selection (`gzip`/`br`/
`zstd`/`*`/`q=0`, direct requests for `.gz`/`.br`); the symlink-to-`secret.php`
trick (`notes.txt.gz -> secret.php`: **refused** — `plain_file` rejects script
targets — plain file served), `secret.php` with `Accept-Encoding: gzip`
(runs the script, never the `.gz`); `evil.svg` served as `image/svg+xml`,
`config.php.bak` served raw — both byte-identical to `php -S` (recorded, not
rated, as in round 1); hidden `.config.php.swp` 404; a FIFO under the docroot
 answered by the front controller without hanging (the `is_file` check runs
before any `open`); case-variant paths on this case-insensitive FS; `--compress`:
8 MB compressible (0.02 s, 8 KB out), 4 MB incompressible, static files over
4 MB not compressed on the fly, ETag weakened to `W/` and `Vary` added.
**BREACH check:** with `--compress`, a 2 KB response reflecting `?r=` next to
a secret compresses to lengths that vary with the guess (74 vs 76 bytes for a
correct prefix) — the compression side-channel is present when the flag is
on. **Default (no `--compress`) never compresses dynamic responses**
(`content-encoding` absent) — the documentation should warn that `--compress`
must stay off (or secrets must not share a response with reflected input)
when BREACH matters. **Found:** none beyond the recorded parity items.

### Area 5 — trusted proxies (`forwarded.rs`)

Tried (~40 min): with no `--trusted-proxies` (default), `X-Forwarded-*` and
`Forwarded` from 127.0.0.1 change nothing (`REMOTE_ADDR` stays the socket
address, no `HTTPS`, `HTTP_HOST` from `Host`); with `--trusted-proxies
private`: rightmost-untrusted chain walk (incl. 5,000-entry chains, IPv6 with
port, all-trusted chains, malformed entries → no claim), duplicate header
lines joined, `X-` taking precedence over `Forwarded` per field, proto/host/
port handling. **Held.** Two recorded notes (not rated): a junk
`X-Forwarded-Proto: junk` from a trusted proxy sets `SERVER_PORT=80` while
`HTTPS` stays absent; duplicate `Host` headers are comma-joined into
`HTTP_HOST`. `--health /health` answers `200 ok` even with 10 infinite-loop
requests in flight (1.0 s), and is checked before routing, so no script path
can shadow it (a file named `health` is shadowed by it — documented opt-in
behaviour).

### Area 6 — the limits (`config.rs`, `connection.rs`, `worker.rs`)

Tried (~1.5 h): request timeout — 10 concurrent `while(true){}` all got 504
at exactly the limit and the server stayed responsive (200 control), a
pipelined request after a 504 was answered 200; silent body → 408 at 30.0 s
with `connection: close`; idle keep-alive closed at 75.0 s; connection cap —
10,050 connections: server RSS 82 MB, control request unanswered while the
cap was full, recovers when they close; slow readers — 10 clients reading 1 KB
of a 20 MB dynamic response pinned 33 MB (round 2: 259 MB) and of a 50 MB
static file 18 MB (round 2: 962 MB) — **F5 fixed by the streaming buffer**;
F3 re-measured: one `grow_bounded.php` request → 417-421 MB peak RSS in 0.9-25 s
(machine load dependent), the unbounded loop reached ~667 MB in 8 s and
keeps growing (~470 MB/s) until the kernel OOM killer ends the process — the
cheapest whole-server kill is still a 3-line `while(true){$a[]=str_repeat('x',4096);}`
request, ~4-5 s to 2 GB; the abort fallout is R3-2 (temp files) and the
session lock/file are correctly released (area 2).
**Found:** R3-4 (residual of F4). F3 still open.

### Area 7 — files and streams from PHP (`crates/extensions/streams/`, `spl/`)

Tried (~50 min): `php://memory`/`php://input`/`php://temp/maxmemory:1024`
(8 MB spill to a temp file, deleted with the request), `data:` URIs, `glob()`
with 30-deep `*/` and 4,000-char patterns (fast, no blowup), NUL in paths →
`ValueError` like PHP, `include` of `data:`, `php://filter`, `http://` → all
refused for inclusion (allow_url_include-off parity), `include` with an
absolute traversal path behaves exactly like PHP (app-level concern), `tmpfile`
deleted at request end, `tempnam` persists by design, `SplTempFileObject`,
`flock` held without unlock is released when the request ends (next request
locks fine). **Held everything; no findings.** `php://filter` chains are
unsupported (fail closed).

### Area 8 — per-request isolation, incl. the shared `NativeTables`

Tried (~50 min): after requests that set `ini_set`, `setlocale`,
`set_error_handler`, `spl_autoload_register`, `putenv`, user-class static
properties, `class_alias` (of both a user class and a native class), `define`,
`strtok`, an un-flushed `ob_start`, `header_register_callback`,
`register_shutdown_function`, `$GLOBALS` — and after a fatal error and after a
timeout-aborted (504) request — the next request on the same worker saw fresh
everything (ini defaults, locale C, no handler, no aliases/constants/autoload,
no globals, `headers_list()` empty, `ob_level` 0, own headers). Native classes
are shared per worker via `NativeTables` and resist mutation from PHP:
`ReflectionClass::setStaticPropertyValue` on `Exception`/`ArrayObject` fails
cleanly ("does not have a property named r3" — native classes carry no static
slots), aliases of native classes vanish with the request, enum cases are
immutable. **Held everything; no findings.**

### Area 9 — `phera new`

Tried (~30 min): see R3-5. Argument injection into Composer is not reachable
through the CLI (leading `-` rejected, basename validated, no shell); the
symlink target is followed (R3-5).

### Area 10 — error output to a remote client

Tried (~20 min): parse-error 500, uncaught exception 500, runtime-error 500.
**Today's behaviour (F7 stands, and is a bit worse than round 2 noted):**
bodies contain the absolute filesystem path, line, and stack trace — and the
full exception *message*, which applications routinely fill with secrets
(verified: `throw new Exception("boom with secret=DB-PASSWORD-xyz")` is echoed
verbatim). `ini_set('display_errors','0')` has **no effect** — `display_errors`
is not an honoured setting anywhere in `crates/`. Comparison: `php -n -S`
sends uncaught exceptions to its stderr with an empty 500 body and shows
parse errors in the body; production PHP-FPM (`display_errors=Off`) sends an
empty 500 and logs server-side. **Safe default for `phera serve`:** generic
500 body to the client (like production PHP), full diagnostic to the server
log, with a `display_errors`-style switch defaulting to off under `serve` and
on on the command line.

---

## Status of the round-1/round-2 findings (rerun on this build, 2026-11-19)

| # | Status | Evidence (3/3 fresh servers each unless noted) |
| --- | --- | --- |
| F1 source disclosure via case-variant extension | **fixed and verified** | `repro_f1_source_disclosure.py` ok: `/secret.PHP` runs the script. |
| F2 script-set `Content-Length` framing | **fixed and verified** | `repro_f2_content_length_framing.py` ok: 101 declared = 101 on the wire. |
| F3 no per-request memory limit / execution timeout | **still present** | `repro_f3_no_memory_limit.py` VULNERABLE: 417-421 MB peak from one request; unbounded loop ~667 MB in 8 s, ~470 MB/s until OOM. Execution timeout now exists (504), memory limit still does not. |
| F4 no body/idle timeout, no connection cap | **partially fixed; residual = R3-4** | silent body → 408 at 30.0 s; idle keep-alive closed at 75.0 s; cap 10,000/worker works (10,050 conns → control stalls, 82 MB RSS). But a 1 B/5 s trickle is held past 100 s (R3-4). |
| F5 buffered responses pinned by slow readers | **fixed and verified** | `repro_f5_buffered_response_pinning.py` ok: 5×50 MB → 13 MB peak; 10×20 MB dynamic → 33 MB. |
| F6 `$_POST` on non-POST methods | **fixed and verified** | `repro_f6_post_on_get.py` ok. |
| F7 error bodies leak absolute paths | **still present, no off switch** | `repro_f7_error_path_leak.py` VULNERABLE; today also exception messages (see area 10); `display_errors` not honoured. |
| F8 `$_POST` for case-variant method tokens | **fixed and verified** | `repro_f8_lowercase_post_method.py` ok: `post`/`Post`/`pOsT` give `$_POST=[]`. |
| F9 duplicate `Content-Type` split decision/report | **fixed and at PHP parity** | `repro_f9_duplicate_content_type.py` ok; today php -S joins duplicates the same way and fills `$_POST` identically. |
| engine: `strict-identity-reference-cycle-crash` | **fixed and verified** | prints `bool(true)`, exit 0, matches PHP. |
| engine: `selfref-array-cursor-panic` | **fixed and verified** | `ok`, exit 0 for all cursor ops. |
| engine: `long-chain-assign-clone-stack-overflow` | **still present** | exit 134 (stack overflow, abort) on the 600k-term file; PHP exits 255 with a clean fatal. |
| engine: `class-chain-linking-quadratic` | **still present, better numbers** | n=20,000: Phera 2.9 s / 1.9 GB vs PHP 0.08 s / 37 MB (round 1: 16.1 s / 4.4 GB). |
| engine: `no-memory-limit-unbounded-ram` | **still present** | ~667 MB in 8 s and climbing (watchdog-killed); PHP dies at 128 MB in 0.06 s. Same root cause as F3. |

## Fuzzing totals (round 3)

Mutation fuzzer `tests/security/http/fuzz.py` against freshly started 1-worker
servers on `127.0.0.1:18490-18492`, `nice -n 15`, three 240 s chunks (seeds
3101-3103): **4,089 requests over 720 s of fuzzing**. Status distribution:
`200`: 1,058, `400`: 2,395, `404`: 139, `405`: 1; anomalies `empty-response`:
496 (hyper closing connections that died mid-parse without a 400 body —
correct behaviour, logged server-side). **No panic, no exit, no stopped
accept loop; every server answered its liveness control at the end of its
chunk.** The round-1/2 attack batteries (`attack_paths`, `attack_parsing`,
`attack_headers`, `attack_superglobals`, `attack_isolation`,
`attack_resources`) were also rerun end to end via `tests/security/http/run_all.sh`:
same results as round 2 (no traversal, no header injection, isolation clean —
the battery's `headers_list` "problems" are its own expectation not knowing
Phera's default `Content-Type`; all state values are fresh), no panics in any
server log.
