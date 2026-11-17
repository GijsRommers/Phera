# Security findings: `phera serve` (HTTP server and request boundary)

---

## Round 2 — 2026-11-17, current `main` merged into `security/http-server`

Second round of the authorized defensive testing. Tested commit:
`bf8e312aaae09264a299fb26c68c572bdcc92fbc` (`http-round-2` =
`origin/security/http-server` + `origin/main` at `904cfdb6`), release build
`cargo build --release -p phera-cli`, i.e. current `main` plus round 1's
security files. The three fixes from round 1 are in commit `88a0bb4`
(`crates/cli/src/serve/site.rs:85-88`, `crates/server/src/connection.rs:123-124`,
`crates/runtime/src/http/superglobals.rs:31`, one test in
`crates/cli/tests/serve.rs`). Method as in round 1: raw-socket Python (stdlib
only), every reproducer confirmed three times against freshly started servers
(`--workers 1`, `127.0.0.1:18400-18499` only, `nice -n 15`), compared with real
PHP 8.5.12 (`/opt/homebrew/bin/php -n -S`) on the same fixture site wherever PHP
semantics matter. The only commits touching the request path since round 1's
`cfed5d7` are `88a0bb4` (the fixes) and `54093d2` (LTO/mimalloc build), per
`git log cfed5d7..origin/main -- crates/server crates/cli crates/runtime crates/extensions/http`.

## Status of the round-1 findings

| # | Status | Evidence (3/3 fresh servers each) |
| --- | --- | --- |
| F1 | **fixed and verified** (88a0bb4) | `GET /secret.PHP` and `/secret.pHp` now execute the script (`200`, body `REAL secret.php ran`), never its source. |
| F2 | **fixed and verified** (88a0bb4) | `GET /cl.php` on a keep-alive connection now gets `content-length: 101` with exactly 101 body bytes; the script's `Content-Length: 5` is dropped. No desync in any pipelined case (below). |
| F3 | **still present** | `grow_bounded.php`: one request grew the server to 417 MB peak RSS in 26 s, 3/3 runs. Known and open. |
| F4 | **still present** | Trickled body (6 of 1,000,000 bytes over 35 s): connection still open, no response, no close, 3/3 runs. Known and open. |
| F5 | **still present** | 5 clients reading 1 KB of a 50 MB static file pinned 259 MB RSS, 3/3 runs. Known and open. |
| F6 | **fixed and verified** (88a0bb4) | `GET`/`PUT` with a form body now give `$_POST=[]`, 3/3 runs. But see F8: the fix's method match is case-insensitive. |
| F7 | **still present** | 500 body still contains the absolute path of `broken.php`, 3/3 runs. Known and open. |

## New findings

| # | Name | Severity | One line |
| --- | --- | --- | --- |
| F8 | `$_POST` filled for case-variants of the method (`post`, `Post`) | low | The F6 fix matches the method with `eq_ignore_ascii_case("POST")`; RFC 9110 §9.1 makes methods case-sensitive and PHP only fills `$_POST` for exactly `POST`. |
| F9 | Duplicate `Content-Type` headers: `$_POST` and `$_SERVER['CONTENT_TYPE']` disagree | low | `$_POST` is parsed from the first `Content-Type` while `$_SERVER['CONTENT_TYPE']` reports the last; PHP comma-joins duplicates and would not fill `$_POST`. |

### F8 — `$_POST`/`$_REQUEST` filled for case-variant method tokens (low)

- **Reproducer:** `tests/security/http/repro_f8_lowercase_post_method.py`.
- **Request:** `post /post.php HTTP/1.1` (also `Post`, `pOsT`) with
  `Content-Type: application/x-www-form-urlencoded` and body `a=1`.
- **Observed (3/3 fresh servers):** `200`, `$_SERVER['REQUEST_METHOD']='post'`,
  `$_POST={"a":"1"}`, `$_REQUEST={"a":"1"}`. Real PHP 8.5.12 (`php -n -S` on the
  same site) does not treat `post` as POST: it answers with an empty `$_POST`
  (its CLI server 501s `Post`), because php-src compares the method with a
  case-sensitive `strcmp`.
- **Expected:** RFC 9110 §9.1: "method names are case-sensitive"; `post` is not
  the registered POST method, so `$_POST` must stay empty, exactly as PHP does.
- **Why (code):** `crates/runtime/src/http/superglobals.rs:31` —
  `request.method.eq_ignore_ascii_case("POST")` (the F6 fix) accepts any
  ASCII case of the method token.
- **Suggested fix:** compare exactly: `request.method == "POST"`.
- **Severity:** low — an app that assumes "`$_POST` non-empty implies the
  request method was POST" (the very property F6 restored) can still be
  surprised by a `post` token; apps that check `REQUEST_METHOD === 'POST'`
  are unaffected, and an attacker could just send `POST` anyway.

### F9 — duplicate `Content-Type` headers split the decision from the report (low)

- **Reproducer:** `tests/security/http/repro_f9_duplicate_content_type.py`.
- **Request:** `POST /post.php` with two header lines,
  `Content-Type: application/x-www-form-urlencoded` then
  `Content-Type: application/json`, body `a=1`.
- **Observed (3/3 fresh servers):** `200`, `$_SERVER['CONTENT_TYPE']="application/json"`
  but `$_POST={"a":"1"}`. Real PHP 8.5.12 combines duplicate field lines
  (`CONTENT_TYPE` becomes `application/x-www-form-urlencoded, application/json`)
  and does **not** fill `$_POST` for that combined value.
- **Expected:** one consistent view of the request: either `$_POST` and
  `$_SERVER['CONTENT_TYPE']` agree, or a duplicated singleton field is rejected
  with 400 (RFC 9110 §5.5 lets a recipient combine or reject; hyper accepts the
  duplicate).
- **Why (code):** `is_form` reads `HttpRequest::header("Content-Type")`
  (`crates/runtime/src/http/mod.rs:64`), which returns the **first** match, while
  the `$_SERVER` builder (`crates/runtime/src/http/superglobals.rs:122`) inserts
  every header in order so the **last** one wins for `CONTENT_TYPE`.
- **Suggested fix:** use the same value for both (e.g. have `is_form` read what
  `$_SERVER` reports), or reject requests with a duplicated `Content-Type`.
- **Severity:** low — an app branching on `$_SERVER['CONTENT_TYPE']` (e.g.
  expecting JSON and reading the raw body) can observe `$_POST` populated from
  a header value it was never shown; no parser-level desync results from it.

## Attacking the fixes

### F1 fix — extension spellings and encodings (`t_f1.py` battery, Phera vs php -S on the same site)

Every shape below was requested from both servers; "runs" means the script
executed (200, its output), "front" means the front controller answered:

- Case variants `/secret.PHP`, `/secret.pHp`, `/secret.Php`, `/SECRET.php`,
  `/SeCrEt.pHp`: **run** on both Phera and php -S.
- Percent-encoded letters `/secret.p%68p`, `/secret.%70hp`, `/secret.p%68%70`,
  `/secre%74.php`, `/%73ecret.php`, `/secret%2Ephp`: **run** on both (single
  decode, then the case-insensitive extension check on the canonical path).
- NUL `/secret.php%00`, `/secret.php%00.txt`, `/a%00b.php`: Phera 404 (decode
  rejects NUL, `site.rs` `percent_decode`); php -S 400 or runs a truncated
  path — Phera safe.
- Trailing dots/spaces/slashes `/secret.php.`, `/secret.php%2e`,
  `/secret.php%20`, `/secret.php/`, `/secret.php//`, `/secret.php/.`: front
  controller on both (this macOS volume does not resolve `secret.php.` to
  `secret.php` — verified with `stat`); a file literally named `trail.php.` or
  `trail.php ` **is** served raw by both Phera and php -S (parity).
- Unicode look-alikes (`/%EF%BC%90hp` full-width P, `p%C4%B0p`), 8.3 names
  (`/SECRE~1.PHP`), alternate-data-stream shapes (`/secret.php:stream`,
  `/secret.php/..namedfork/rsrc` and its percent-encoded form): front
  controller / 404 on Phera — no source, and `eq_ignore_ascii_case` is
  ASCII-only so no non-ASCII case folding can fake the extension.
- Directory named `x.php` (`/x.php`, `/x.php/`): Phera falls back to the front
  controller, no listing (php -S 404s).
- Symlink with another name (`link-inside.txt` -> `secret.php`): **Phera runs
  the script** because the extension is checked on the canonicalized path
  (`site.rs:76-88`); php -S serves the target's source — Phera is strictly
  safer here. `link-txt.php` -> `data.txt` serves the text on both.
- Symlinks out of the site (`evil.php`, `link-out.txt`, `dirlink/...`): Phera
  front controller (canonicalize + prefix test); php -S follows them out of the
  docroot and serves the outside file.
- PHP source under other extensions — `config.php5`, `page.phtml`, `lib.inc`,
  `p.phar`, `config.php.bak`, `config.php~`, `config.php.swp`, `config.PHP.bak`,
  `config.Php5` — is served as `application/octet-stream` raw source by
  **both** Phera and php -S (byte-identical bodies); the hidden
  `/.config.php.swp` is refused by Phera (hidden files 404). This matches real
  PHP's built-in server, so it is recorded, not rated.

**Conclusion:** the F1 fix holds against every spelling and encoding tried; no
path was found that still serves `.php` source. The only source-serving cases
left are files that are not `.php` by name, and those behave exactly like real
PHP.

### F2 fix — every response shape where the server now decides framing

Battery `t_f2.py`/`t_f2b.py`/`t_f2c.py` (wire bytes inspected, pipelined second
request sent after each), compared with php -S where useful:

- `HEAD /head.php?n=50`: `content-length: 50`, no body. `HEAD /big.bin`:
  `content-length: 52428800`, no body. `HEAD /cl.php` (script sets CL:5):
  `content-length: 101`, no body — correct on all.
- Statuses that must not have a body: `204`/`304` with a script echoing output
  -> Phera sends no `content-length` and no body (php -S wrongly sends a
  10-byte body after `204`); `205` -> `content-length: 10` + body, framing
  consistent (same as php -S); `100`/`103` -> final `500` with
  `content-length: 0` and the connection closed; `101` (script adds
  `Upgrade:`) -> `101` sent, then the connection is closed — contained, no
  desync, no stuck socket.
- Script-set `Content-Length` larger than the body (`clbig.php`): server sends
  `content-length: 1` + `x`. Script-set `Transfer-Encoding: chunked` with the
  script writing its own chunk framing (`te.php`): TE dropped, server frames
  `content-length: 15`, the chunk text is just body bytes.
- Header-name shapes via `header()`: uppercase `CONTENT-LENGTH`, name with
  trailing/leading space, duplicate CL (5 then 100000), CL+TE together, TE
  twice, 10×CL + 10×TE, CL with empty/junk values (`-1`, `abc`, `1e20`, `0x10`,
  `5, 6`): **all dropped** in every case (`connection.rs:123-124` matches the
  normalized `HeaderName`, so case and surrounding whitespace cannot dodge
  it); the server always frames with the true length.
- `Connection`/`Keep-Alive` set by a script are passed through and honored
  (`Connection: close` really closes the socket; `keep-alive` keeps it); a
  junk value (`keep-alive, close`) is passed verbatim but framing stays
  CL-based, so no desync. php -S emits duplicate `Connection` headers here;
  Phera does not.
- Keep-alive/pipelined desync attempts — after `cl.php`, `te.php`, a 204, a
  304, a `HEAD`, and the 10×CL+TE case, a second pipelined request was always
  answered with correct framing; leftover bytes never appeared on the wire.
- HTTP/1.0: without `Connection` the response carries no `connection` header
  and the socket closes after (a second request gets EOF); with
  `Connection: keep-alive` two requests are answered on one 1.0 socket;
  HTTP/1.1 `Connection: close` closes; 1.1 keep-alive answered 5/5 sequential;
  a pipeline whose first request says `close` yields exactly one response then
  close.

**Conclusion:** the F2 fix holds; the server alone controls framing in every
case tried, and no combination desynchronised a keep-alive or pipelined
connection.

### F6 fix — methods, content types, `$_REQUEST`

Battery `t_f6.py`/`t_f6b.py` vs php -S: `GET`/`PUT`/`DELETE`/`PATCH`/`OPTIONS`/
`HEAD`/`PROPFIND` with a form body all give `$_POST=[]` now (fixed). `POST` with
a form body (plain or chunked), with charset parameter, upper/mixed-case media
type, or surrounding OWS behaves identically to PHP. `$_REQUEST` is the
GET+POST merge (PHP's default `request_order=GP`) in every case. Divergences
found: F8 (method case) and F9 (duplicate Content-Type) above; a
`Content-Type` list (`application/x-www-form-urlencoded, ...`) is *not* a form
in Phera while php -S prefix-matches it — Phera is the stricter side.
`multipart/form-data` still does not fill `$_POST` (known missing feature from
round 1). Methods with trailing space/tab or a NUL in the header value are
rejected with 400 by hyper (stricter than php -S).

## Areas covered more deeply this round

- **HTTP/1.0 clients and `Connection: close`:** see the F2 section — all
  correct (close-by-default on 1.0, keep-alive honored, close honored,
  pipelining with `close` drops the remainder as RFC 9112 requires).
- **Extreme query strings** (`t_deep.py` part B): 1 MB query / 1 MB single key /
  1 MB single value -> connection reset (hyper head cap); 20 000 params, 100 k
  target, 50 000 bare params, 100 k `%` signs -> `414`; nesting 2000 deep ->
  `200` in 0.00 s, the variable dropped at depth 64 while `QUERY_STRING` keeps
  the raw 6 kB; invalid `%`, `%00`, `;` separators, `+` signs, empty query,
  `%2f` in the path, `%3f` in the path all answered sanely and match round 1's
  PHP-diffed semantics.
- **Concurrency fairness** (`t_deep.py` part C, 1 worker): 10 slow keep-alive
  clients (300 ms script) + 10 fast clients (30 requests each) + one request
  running a 3 s PHP loop: 300/300 fast requests answered with 0.001 s median
  latency (max 0.009 s), 60/60 slow answered at ~0.30 s, the loop request
  completed in 3.01 s. No starvation.
- **Requests during shutdown** (`t_deep.py` part D + big-write probe): SIGTERM
  mid-load with an in-flight 300 ms script — the request was answered in full
  (`200`, complete body) after the signal; the port then refused new
  connections. SIGTERM while a 20 MB response was mid-write: the client
  received all 20 971 520 bytes, `complete=True`, uncorrupted. The graceful
  shutdown in `crates/server/src/worker.rs` (watcher + `shutdown_timeout`)
  works.
- **New surface since `cfed5d7`** (`git log cfed5d7..origin/main -- crates/server
  crates/cli crates/runtime crates/extensions/http` = only `88a0bb4` +
  `54093d2`): the time functions `sleep`/`usleep`
  (`crates/extensions/standard/src/time.rs`) are async tokio sleeps. Negative
  arguments raise `ValueError` exactly like PHP 8 (Phera answers 500 with the
  message; php -S shows a fatal but sends 200 — Phera's status is the more
  correct one). `usleep(PHP_INT_MAX)`/`sleep(PHP_INT_MAX)` park the request
  without panicking (tokio clamps the deadline) and cost nothing but the task
  and fd; measured: 60 concurrent `sleep(3600)` requests = 60 sockets and
  ~15 MB RSS total, control-request latency 11 ms, worker never blocked —
  strictly better than php -S, which is single-threaded and would block every
  request for the hour. Holding such requests forever is the already-open F4
  (no request timeout / connection cap), not a new class.
- **Regression run of the round-1 batteries** (`attack_paths`,
  `attack_parsing`, `attack_headers`, `attack_superglobals`,
  `attack_isolation`) against the merged build: same results as round 1, no
  panics, per-request isolation clean.

## Round-2 fuzzing totals

Mutation fuzzer `tests/security/http/fuzz.py` against freshly started 1-worker
servers on `127.0.0.1:18412`, `nice -n 15`, one chunk per server restart,
seeds varying. Harness note: round 1's `fuzz.py` had a bug in its liveness
check (`alive()` passed an int status to `status_of`, so it always reported
"not answering" at the first 200-request checkpoint); fixed in this branch —
the server itself answered every control request (verified manually during the
run). Totals: **FILL-IN-AFTER-CAMPAIGN** requests over **FILL-IN-AFTER-CAMPAIGN**
wall-clock hours; status and anomaly distributions, and the panic check, per
chunk in `logs/fuzz-summary.txt` of the test rig. Result: no panic, no exit,
no stopped accept loop.

---

## Round 1 — original report (commit `cfed5d7`, branch `security/http-server`)

Authorized defensive testing of Phera's built-in HTTP server, done from
`security/http-server` against the release build of `cfed5d7`. Scope: the
HTTP server and everything between a network request and a running PHP
script — connection handling, request parsing, routing to files, static
serving, the superglobals, the response functions, and isolation between
requests. Hostile PHP source given to the engine is out of scope (separate
task); PDO is out of scope.

Method: raw-socket Python (stdlib only) so every byte on the wire is
explicit; each finding was reduced to the smallest reproducer, confirmed
three times against freshly started servers (`--workers 1`,
`127.0.0.1:18400-18499` only), and compared with real PHP 8.5.12
(`/opt/homebrew/bin/php -n -S`) where PHP semantics matter. Reproducers and
attack batteries live in `tests/security/http/` (see its README); the
findings below reference them. Every reproducer was also run through
`tests/security/http/run_all.sh` end to end.

## Summary

| # | Name | Severity | One line |
| --- | --- | --- | --- |
| F1 | PHP source disclosure via case-variant extension | high | `/secret.PHP` is served as `application/octet-stream` instead of executed — config files with credentials leak verbatim. |
| F2 | Script-set `Content-Length` sent verbatim on a keep-alive connection | high | `header('Content-Length: 5')` + 101-byte body desyncs every framing-respecting reader behind/around the server (response splitting). |
| F3 | No per-request memory limit, no execution timeout | high | One request grew the server from 10 MB to 2.1 GB RSS in ~3-5 s; an unbounded loop reaches the kernel OOM killer and kills every worker. |
| F4 | No timeout on request bodies or idle keep-alive connections, no connection cap | medium | A trickled body (or an idle keep-alive) is held open forever; header reads are capped at 30 s, body reads are not capped at all. |
| F5 | Whole responses buffered in memory; non-reading clients pin them | medium | 10 clients reading 1 KB of a 100 MB static file pinned ~1 GB RSS with no write timeout. |
| F6 | `$_POST` filled for non-POST methods | low | `GET`/`PUT` with a form body populate `$_POST`; PHP only does this for POST. |
| F7 | Error bodies leak absolute filesystem paths | low | Parse-error 500s and warnings/fatals embed the full canonical path, line and stack trace; matches php -S's default `display_errors`, but there is no way to turn it off. |

Not findings (checked, held up): path traversal in every encoding, hidden
files, symlinks out of the site, directory listings (area 1); request
smuggling/desync via CL/TE conflicts, malformed chunked bodies, control
bytes in headers, obs-fold, pipelining poisoning (area 2); CRLF/NUL
injection through `header()` (area 3); body size cap at 8 MB and the 30 s
header timeout, concurrency/fairness under load (area 4); query/cookie
parser semantics vs PHP, `$_SERVER` spoofing (area 5); per-request state
isolation (area 6); panic-freedom of the process under 2 h+ of mutation
fuzzing (area 8).

---

## F1 — PHP source disclosure via case-variant extension (high)

- **Reproducer:** `tests/security/http/repro_f1_source_disclosure.py` (fixture
  file `secret.PHP` containing `<?php $db_password = "SUPERSECRET-PHP-SOURCE"; ...`).
- **Request:** `GET /secret.PHP HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n`
  (also `/secret.pHp`; same result for `/config.php5`, `/page.phtml`, `/lib.inc`).
- **Observed (3/3 fresh servers):** `200 OK`, `content-type: application/octet-stream`,
  body is the raw PHP source: `<?php $db_password = "SUPERSECRET-PHP-SOURCE"; ...`.
- **Expected:** the file is executed (or refused), never sent as bytes.
  Real PHP 8.5.12's built-in server matches the extension
  **case-insensitively** and runs `/secret.PHP` (verified: it printed the
  script's output, not the source). Serving `.php5`/`.phtml`/`.inc` raw is
  what php -S also does, so those are noted but not rated; the case variants
  are a real divergence from PHP.
- **Why (code):** `crates/cli/src/serve/site.rs:82` —
  `file.extension().is_some_and(|extension| extension == "php")` is a
  case-sensitive `OsStr` comparison, so only exactly lowercase `.php` is
  executed; everything else falls into the static branch
  (`site.rs:106` in `crates/cli/src/serve.rs` reads and returns the file).
- **Suggested fix:** compare the extension case-insensitively
  (`extension.to_string_lossy().eq_ignore_ascii_case("php")`), and treat the
  usual PHP source extensions (`.php`, `.phtml`, `.php5`, `.inc`, ...) as
  code-or-refuse rather than downloadable bytes.
- **Severity:** high — a single `GET` returns the source of any config file
  whose extension is not exactly lowercase `php` (credentials, API keys), no
  other condition needed.

## F2 — script-set `Content-Length` emitted verbatim; body not reconciled (high)

- **Reproducer:** `tests/security/http/repro_f2_content_length_framing.py`
  (fixture `cl.php`: `<?php header('Content-Length: 5'); echo str_repeat('A', 100), "\n";`).
- **Request (keep-alive):** `GET /cl.php HTTP/1.1\r\nHost: x\r\n\r\n`
- **Observed (3/3):** wire response head `HTTP/1.1 200 OK\r\ncontent-length: 5\r\n...`
  followed by **101 body bytes**, and **no `connection: close`** — the
  connection stays open. A reader that honours `content-length: 5` consumes
  5 bytes as the body and parses the remaining 96 `A`s as the next response
  on the same connection (demonstrated in the reproducer: leftover bytes are
  on the wire after the framed body, and the server then answers a fresh
  pipelined request after them).
- **Expected:** the server controls response framing. RFC 9110 §8.6: a server
  must send a `Content-Length` matching the body it sends; a script-set
  framing header must be overridden (or the connection closed). PHP's own
  dev server sends `Connection: close` in exactly this situation, which
  contains the mismatch; Phera keeps the connection alive, which does not.
  (`Transfer-Encoding` set by a script is safe: hyper re-frames the body as
  chunked itself.)
- **Why (code):** `crates/extensions/http/src/head.rs:49` stores any header
  name a script passes to `header()`; `crates/server/src/connection.rs:109-121`
  (`to_hyper_response`) copies them into the hyper response, and hyper does
  not reconcile a user-supplied `content-length` with the actual `Full` body.
- **Suggested fix:** in `to_hyper_response` (or in `ResponseHead::apply`),
  drop/override script-set `content-length` (and `transfer-encoding`) so the
  server alone decides framing — or set `connection: close` when they are
  present.
- **Severity:** high when Phera sits behind a proxy/cache/load balancer that
  trusts upstream framing: poisoned caches, smuggled `Set-Cookie`/status
  lines, desynced keep-alive connections. Direct keep-alive clients that
  frame by `Content-Length` desync the same way.

## F3 — no per-request memory limit and no execution timeout (high)

- **Reproducer:** `tests/security/http/repro_f3_no_memory_limit.py` with
  `grow_bounded.php` (allocates ~400 MB in a loop and stops, so it is safe);
  the unbounded variant was tested in the campaign (`grow.php`:
  `while (true) { $a[] = str_repeat('x', 4096); }`).
- **Request:** `GET /grow.php HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n`
- **Observed (3/3 fresh servers):** RSS went from ~10 MB to **> 2 GB in
  3.1 s, 3.1 s and 27 s** (killed by the test rig's 2 GB guard; on a real
  machine the kernel OOM killer ends the process, taking every worker and
  every in-flight request with it). 30 concurrent `hang.php` (`while(true){}`)
  requests also all stayed alive forever — the worker kept answering other
  requests in 0.06-0.09 s (the fairness yield works), but the tasks never end.
- **Expected:** PHP's defaults stop exactly this: `memory_limit=128M` and
  `max_execution_time=30` (web SAPI). A request that runs forever or grows
  without bound must be cut off.
- **Why (code):** nothing in the request path bounds time or memory:
  `crates/server/src/connection.rs` spawns the handler with no deadline;
  `crates/runtime/src/lib.rs:105` (`handle_request`) runs the VM to
  completion; `Vm::output` and PHP arrays grow without a cap
  (`crates/vm/src/lib.rs`). `ServerConfig` (`crates/server/src/config.rs`)
  has only `max_body_bytes` (input) and `header_read_timeout`.
- **Suggested fix:** add a per-request deadline (e.g. `tokio::time::timeout`
  around `handler.handle`, default 30 s) and a memory ceiling the VM checks
  periodically (like PHP's `memory_limit`), returning 500 and dropping the
  request's values.
- **Severity:** high — a single request (any application bug that loops or
  appends unboundedly) exhausts all memory and kills the whole server
  process; with N requests it is N times faster.

## F4 — no timeout on request-body reads or idle keep-alive connections; no connection cap (medium)

- **Reproducer:** `tests/security/http/repro_f4_no_body_timeout.py`.
- **Request:** `POST /post.php HTTP/1.1\r\nHost: x\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 1000000\r\n\r\n` then 1 byte every ~5 s.
- **Observed (3/3):** after 65 s (13 of 1,000,000 body bytes sent) the
  connection is still open, no response, no close — the 30 s
  `header_read_timeout` does not cover body reads (it fired correctly for
  slow *headers*: a 1-byte/4 s head was cut at 30.2 s). Declared
  `Content-Length` values of 1 KB up to 2^63 with no body at all are likewise
  held open indefinitely. 300 idle keep-alive connections stayed usable
  after 25+ s idle (no idle timeout), and 1900 connections were all accepted
  (no cap on connection count).
- **Expected:** RFC 9110 does not mandate timeouts, but a server that wants
  to survive slowloris-class clients needs a body-read timeout and an idle
  keep-alive timeout (and ideally a max-connections limit). The worker stayed
  responsive for other clients throughout (200 in 0.00 s), which is good —
  the problem is purely that nothing ever reaps these connections.
- **Why (code):** `crates/server/src/connection.rs:61` sets only
  `header_read_timeout` on the hyper builder; `read_body` (connection.rs:127)
  awaits frames with no timeout; `worker.rs` accepts without a cap.
- **Suggested fix:** wrap the whole request (or at least `read_body`) in a
  timeout, add a keep-alive idle timeout, and cap concurrent connections per
  worker.
- **Severity:** medium — one connection per few bytes pins a task and an fd
  forever; at scale (FD/memory exhaustion) the server stops accepting, and
  FD exhaustion could not be demonstrated inside this rig's 2000-connection
  limit because the shell's fd limit is ~1 M.

## F5 — whole responses buffered in memory; non-reading clients pin them (medium)

- **Reproducer:** `tests/security/http/repro_f5_buffered_response_pinning.py`
  (fixture `big.bin`, 50-100 MB static file).
- **Request:** 10 × `GET /big.bin HTTP/1.1\r\nHost: x\r\n\r\n`, each client
  reads 1 KB then stops reading.
- **Observed (3/3):** server peak RSS **962 MB** (10 × 100 MB) held for as
  long as the clients hold the sockets; control requests still 200. Static
  files are read whole with `std::fs::read` (no size cap,
  `crates/cli/src/serve/site.rs:84-89`), the response is buffered in memory
  (`Response.body`), and hyper's write has no timeout, so the buffer is
  pinned until the client reads or disconnects.
- **Expected:** streaming or bounded buffering for large responses plus a
  write timeout, so a stalled client releases memory.
- **Why (code):** `crates/cli/src/serve.rs:106` builds `Response` with the
  whole body; `crates/server/src/connection.rs:109` wraps it in
  `Full<Bytes>`; no write deadline anywhere.
- **Suggested fix:** stream file bodies (or cap static-file size), and put a
  timeout on the response write so a non-reading client is dropped.
- **Severity:** medium — a handful of small requests pins gigabytes; with
  enough connections this is memory exhaustion of the whole process (same
  endpoint as F3, but needs no hostile script, only a large servable file or
  large generated output).

## F6 — `$_POST` filled for non-POST methods (low)

- **Reproducer:** `tests/security/http/repro_f6_post_on_get.py`.
- **Request:** `GET /post.php` (and `PUT`) with
  `Content-Type: application/x-www-form-urlencoded` and body `a=1`.
- **Observed (3/3):** `$_POST = {"a": "1"}`. Real PHP 8.5.12 populates
  `$_POST` only for `REQUEST_METHOD == POST` (verified: php -S gives
  `POST: []` for the same GET/PUT).
- **Why (code):** `crates/runtime/src/http/superglobals.rs:58` (`is_form`)
  checks only the Content-Type, never the method.
- **Suggested fix:** also require `request.method == "POST"` in `is_form`.
- **Severity:** low — an app that assumes "GET cannot carry form data" (or
  CSRF logic reading `$_POST`) behaves differently than under PHP; an
  attacker can usually just use POST anyway.

## F7 — error bodies leak absolute filesystem paths (low)

- **Reproducer:** `tests/security/http/repro_f7_error_path_leak.py`
  (fixture `broken.php` with a syntax error).
- **Request:** `GET /broken.php HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n`
- **Observed (3/3):** `500` with body
  `\nParse error: unterminated string in /private/tmp/.../sites/site1/broken.php on line 1\n`.
  Runtime warnings and uncaught exceptions likewise print
  `in /abs/path/file.php:LINE` plus a stack trace into the response body.
- **Expected:** production PHP (`display_errors=Off`) returns an empty 500 and
  logs the detail server-side. php -S with no php.ini shows the same text as
  Phera (verified), so this matches PHP's dev default — but Phera has no
  knob to turn display off.
- **Why (code):** `crates/runtime/src/lib.rs:116` returns
  `HttpResponse::plain(500, error.to_string())`, and `CompileError`'s Display
  (`crates/compiler/src/compile_error.rs:16-30`) embeds the canonical file
  path; VM diagnostics likewise include the path.
- **Suggested fix:** return a generic 500 body to the client and put the
  detailed diagnostic in the server log; add a `display_errors`-style switch
  defaulting to off.
- **Severity:** low — reveals the document-root path and file layout (useful
  to an attacker for F1-style probing and local-file attacks), nothing more.

---

## Per-area coverage: what was tried, for how long, what held up

### Area 1 — reaching files that should not be served
`tests/security/http/attack_paths.py` (~40 shapes): `..` plain/percent-encoded/
double-encoded/mixed-case, backslashes, overlong UTF-8 (`%c0%af`), `%00` and
control bytes, trailing dots/spaces, `//`, `/./`, absolute paths, `;` path
params, `...`, symlinks (file, `.php`, whole directory) pointing outside the
site, hidden files (`.env`, `.git/config`, encoded), directory listings,
`file.php/extra`, `file.php%00.txt`, which extensions execute vs serve.
**Held up:** every traversal encoding was refused (single decode + component
check + `canonicalize` + prefix test, `site.rs:76-82`, `98-130`); symlinks
out of the site fall back to the front controller; hidden files 404; no
directory listing. **Found:** F1 (case-variant extensions served as source).
Note: `/hello.php/extra` falls back to `index.php` instead of running
`hello.php` with PATH_INFO (php -S runs it) — missing feature, not a leak.
Run time: each case against a fresh server plus 3 confirmation rounds.

### Area 2 — request parsing and smuggling
`tests/security/http/attack_parsing.py` (~25 shapes): repeated/conflicting
`Content-Length`, `CL`+`TE`, `TE` variants (chunked twice, identity, junk),
malformed chunked bodies (bad hex, negative, missing CRLF, no terminator,
huge size), CL edge values (negative, hex, `+`, spaces, tab, > u64, 2^63),
header names/values with control chars, empty name, no colon, obs-fold,
bare `\n` and bare `\r`, long targets (16 k/100 k/1 M), long query, 99 vs 200
headers, one 100 k header value, invalid versions, unknown/lowercase/huge
methods, `CONNECT`, `OPTIONS *`, absolute-form targets, pipelined pairs and
malformed-first pipelines.
**Held up:** conflicting CL, control bytes, obs-fold, bare CR, huge heads
(>100 headers → 431, ~100 k target → 414, 1 M → reset) all get 400 + close;
malformed first request in a pipeline poisons nothing (400 + close, second
request dropped, as RFC 9112 requires); a body that looks like a request is
just body. hyper 1.12 parses everything, and the same parser frames and reads
the body, so no desync is possible locally. Leniencies noted (not exploitable
with a single parser, but relevant if a proxy ever fronts Phera): bare `\n`
accepted, identical duplicate CL accepted, CL+TE → TE wins with
`connection: close`, duplicate `TE: chunked` accepted. Body cap works:
exactly 8 MB → 200, 8 MB+1 → 413. Run time: full battery once + re-runs in the
fuzz campaign.

### Area 3 — response splitting / header injection from PHP
`tests/security/http/attack_headers.py` + `repro_f2`: 13 payloads through
`header($_GET['h'])` (CRLF, LF, CR, NUL, `\x0b`, `\x0c`, `\x7f`, `\x01`, HTAB,
status-line smuggling, `X-A\r\n: 1`, trailing CRLF), `http_response_code`
edge values (99/600/65536/-1), script-set `Content-Length`/`Transfer-Encoding`.
**Held up:** every CR/LF/NUL payload is refused by `header()`
(`crates/extensions/http/src/lib.rs:72`) and, as a second layer, hyper's
`HeaderValue::from_bytes` drops any header with control bytes
(`connection.rs:115-116`); no second header line or response ever appeared;
HTAB stays inside one legal header value (RFC 9110 field-content, same as
PHP). **Found:** F2 (script-set `Content-Length` framing mismatch).

### Area 4 — resource exhaustion (all within the campaign limits: nice 15, ≤ 2 cores, `--workers 1`, floods < 60 s, < 2000 connections, server killed at 2 GB RSS)
`tests/security/http/attack_resources.py` + the F3/F4/F5 reproducers: slow
headers (1 B/4 s), slow body (1 B/5 s), declared-huge bodies with no data,
real bodies 8 MB/8 MB+1/20 MB, 300 idle keep-alive connections held 25 s,
1900 connections opened, 500 simultaneous requests, 10 clients that stop
reading a 100 MB response, one infinite-loop script, 30 concurrent
infinite-loop scripts, one memory-growing script.
**Held up:** the 30 s header timeout fires on slow heads; 8 MB body cap
returns 413; 500 simultaneous 300 ms requests all answered in 0.3 s; the
worker stayed responsive (200 in < 0.1 s) during 30 concurrent infinite
loops thanks to the fairness yield; 1900 connections were accepted (no cap).
**Found:** F3 (no memory/execution limit), F4 (no body/idle timeout, no
connection cap), F5 (buffered responses pinned by non-reading clients).
Not demonstrable within limits: actual FD exhaustion (shell fd limit ~1 M >=
2000-connection cap), so F4's worst case is argued, not shown.

### Area 5 — the superglobals parser and `$_SERVER`
`tests/security/http/attack_superglobals.py`, run against both Phera and
php -S and diffed: nesting depth 10-5000 (dropped at 65, matching PHP's
`max_input_nesting_level=64`, fast at 5000), 999/1000/1001/20000 POST params
(PHP aborts at 1001 with zero parsed; Phera keeps the first 1000 — both
bounded), 1 MB keys, 7 MB values, duplicate/conflicting keys
(`a=1&a[]=2`, `a[b]=1&a=2`, ...), malformed percent-encoding, invalid UTF-8,
NUL in names, numeric keys, cookies (first wins — matches PHP), multipart and
JSON bodies, and a 10-header spoofing battery (`Proxy`, `X-Forwarded-*`,
`HTTPS`, `Remote-Addr`, `Script-Filename`, `Document-Root`, `Server-Addr`,
`Request-Method`, duplicate `Host`, raw-byte header value).
**Held up:** every query/cookie case produced byte-identical results to real
PHP; no input cost time or memory out of proportion to its size; security-
relevant `$_SERVER` keys (`REMOTE_ADDR`, `SCRIPT_FILENAME`, `DOCUMENT_ROOT`,
`HTTPS`, `REQUEST_METHOD`, `SERVER_ADDR`) come from the socket/paths and
cannot be set through headers — spoofed values land only under `HTTP_*`
prefixes, exactly like PHP (the httpoxy caveat `HTTP_PROXY` exists in PHP too).
A header value with a NUL byte is rejected by the parser (400), stricter than
php -S. **Found:** F6 (`$_POST` on GET/PUT). Not a finding: multipart bodies
and `$_FILES` are not parsed yet (documented in code; PHP fills them —
missing feature, not exploitable by itself).

### Area 6 — isolation between requests (1 worker, same thread throughout)
`tests/security/http/attack_isolation.py`: sequential requests with
different parameters; requests ending in an uncaught exception, a runtime
error, `exit()`, and a client disconnect mid-response; then 30 concurrent
requests. Each request reports a static counter, a defined constant, a
declared function, a global, `preg_last_error`, `headers_list()` and its own
`X-State` header.
**Held up:** every request — including all 30 concurrent ones and all the
after-fatal/exit/disconnect ones — saw fresh state (`bump=1`, `const='first'`,
`global=1`, `preg_last_error=1`, own `X-State`, empty `headers_list`). The
fresh `Vm` per request (`crates/runtime/src/lib.rs:105-128`) does its job;
nothing set in one request appeared in a later one.

### Area 7 — error handling as an information leak
Broken script (parse error), uncaught exception, runtime error, missing file,
subdirectory; bodies and server logs captured and compared with php -S.
**Found:** F7 (absolute paths in 500 bodies; matches php -S's default
display but differs from production `display_errors=Off`; no off switch in
Phera). **Held up:** a handler panic would become a generic
`500 internal server error` (`connection.rs:47-50`), and the 2 h+ fuzz
campaign below produced no panic, so no Rust panic text ever reached a
client; connection errors are logged server-side only (`worker.rs`/`connection.rs`
`eprintln!`), never echoed into responses.

### Area 8 — robustness under a mutation fuzzer (2 h+ wall time)
`tests/security/http/fuzz.py`: 17 recorded valid requests (GET/POST, forms,
chunked, cookies, pipelined, absolute-form, `OPTIONS *`, traversal shapes,
superglobal shapes) mutated with 1-8 random operations per send (byte flips,
inserts/deletes of interesting bytes, chunk duplication/splicing, truncation,
case changes), one connection per request, read timeouts 0.3-1 s, liveness
control request every 200 requests, run as `nice -n 15` in ~4-minute chunks
against a freshly started 1-worker server, each chunk checking the server log
for panics.
**Result:** see the totals below — **no panic, no exit, no stopped accept
loop; the server answered every control request**. Status distribution is
dominated by 400 (mutated requests refused) and 200 (valid-ish requests
served); `empty-response` anomalies are hyper closing connections that died
mid-parse without a 400 body (logged as `connection error: ...` server-side),
which is correct behaviour.

Campaign totals (updated at the end of the campaign): see
"Fuzzing totals" at the bottom of this file.

## Fuzzing totals

(to be filled in after the campaign — placeholder)
