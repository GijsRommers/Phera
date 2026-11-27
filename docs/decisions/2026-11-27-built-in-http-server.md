# A built-in HTTP server for scripts: `Phera\Http\Server`

- Date: 2026-11-27
- Area: Server and requests
- Decided by: Design

## Decision

A script run with `phera app.php` can serve HTTP itself, in the style of Go's
`net/http` and `Bun.serve`:

```php
$server = new Phera\Http\Server('127.0.0.1:8080', workers: 4);
$server->get('/users/{id}', fn(Phera\Http\Request $r) => ['id' => $r->param('id')]);
$server->listen();
```

- **Classes.** `Phera\Http\Server` (route methods `get`, `post`, `put`,
  `patch`, `delete`, `head`, `options`, `any`, `route`, plus `fallback`,
  `onWorkerStart`, `address`, `workers`, `stop`, `listen`),
  `Phera\Http\Request` (`method`, `path`, `uri`, `param`, `params`, `query`,
  `header`, `headers`, `body`, `json`) and `Phera\Http\Response` (`text`,
  `html`, `json`, `bytes`, `empty`, `withStatus`, `withHeader`, `status`,
  `header`, `headers`, `body`). All three are final. They exist on the command
  line only: under `phera serve` the server is `phera serve`.
- **One VM per worker, alive for the whole run.** A request boots nothing:
  the router picks the handler and the worker calls it on its VM with a
  `Request` over the request's own bytes. No `$_SERVER`, `$_GET`, `$_POST`,
  output buffer or `header()` is involved; what the handler returns is what is
  sent. Requests on one worker run one at a time, in arrival order; while one
  waits for I/O the connections keep reading and queueing.
- **Worker 0 is the script's own VM.** `listen()` serves on the VM that ran
  the script. Each further worker is a thread with a VM of its own that runs
  the same script from the top up to its `listen()` call; its output up to
  there is held back (it repeats worker 0's) and shown only if it never
  reaches `listen()`. When the server stops, an extra worker's script ends
  inside `listen()` as after `exit()`, so code after `listen()` runs once.
- **Sockets.** The constructor binds, so `address()` gives the port of
  `:0` before `listen()`. On Linux each worker accepts on its own
  `SO_REUSEPORT` socket on that port; elsewhere the workers share one socket
  (macOS does not spread connections over `SO_REUSEPORT` sockets). The
  connection code (HTTP/1.1, HTTP/2 with prior knowledge, limits, graceful
  drain) is `phera-server`'s, run on the worker's own `LocalSet` through the
  new `phera_server::serve_local`.
- **Router.** A radix tree in flat arrays: nodes in one `Vec`, children by
  `u32` index, all static text in one byte arena (a split only splits a
  range), parameter values as inline byte ranges of the path. Static beats
  `{param}` beats `*wildcard`, with backtracking. A path whose routes are all
  for other methods answers 405 with `Allow`; `HEAD` uses the `GET` route;
  unknown paths go to the fallback or 404; methods the router does not know
  (`PROPFIND`) go to the fallback or 501. Patterns are checked when they are
  added: `{name}` is a whole segment, `*name` ends the pattern, a duplicate
  method and pattern is a `LogicException`.
- **Return values.** A `Response` as it is; a string as `text/plain`; an
  array as JSON; `null` as 204. Anything else is a `TypeError`.
  `Response::json()` and an array return use
  `JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR`
  (what `JSON.stringify` writes); plain values are encoded in Rust without a
  PHP call, values with objects go through `json_encode` for
  `JsonSerializable`. `Request::json()` decodes the body bytes directly, throws
  `JsonException` on bad JSON, and gives `null` for an empty body.
- **Failures.** An uncaught exception or fatal error in a handler is reported
  as PHP reports an uncaught one (new `Context::report_unhandled`), the
  request gets 500, and the worker serves the next request with its state
  intact. `exit()` in a handler ends the program, as in any PHP script: every
  worker stops and the process exits with that status (`exit()` releases the
  frames of the stack it ends, so the worker could not go on anyway).
- **Stopping.** SIGINT or SIGTERM, or `$server->stop()` from any worker,
  stops every worker; in-flight requests finish first.

## Why

Snapshot and fork (`phera serve`) gives every request a fresh copy of a booted
application, which is what PHP frameworks expect. Some programs want the
other model instead: state that lives across requests (a connection, a
prepared statement, a cache in a variable) and no per-request setup at all,
as in Go, Node or Bun. Building it on the existing connection code and the
existing VM keeps it small: about the router, three classes and one loop.

Re-running the script per worker is the only way to give another thread the
same closures: PHP values live on `Rc` and never cross threads, and a fork of
the VM would be a value graph of the first thread.

## Turned down

- **Building `$_SERVER` and friends per request**, or routing `echo` into the
  response: that is `phera serve`'s model; here the return value is the
  response and `echo` writes to the terminal, as `console.log` does in Bun.
- **Several requests at once on one VM.** The VM runs one PHP call stack;
  more concurrency comes from more workers.
- **The acceptor thread of `phera serve`** (one socket, least-connections
  dispatch): it adds a cross-thread hand-over per connection; per-worker
  sockets need none.
- **A `HashMap` per route segment.** Lookups would hash every segment; the
  tree walks the path once and touches dense byte lists.
- **Custom methods in routes** (`route('PURGE', ...)`): the method table is a
  fixed slot per standard method; `fallback()` receives the rest.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Connection serving (`crates/server/src/worker.rs`) | `run_direct` built a runtime and served inline | `run_direct` builds the runtime and calls `serve_direct`, which `phera_server::serve_local` also runs on a caller's thread |
| Listening sockets (`worker.rs`) | `bind_listener` only | plus `bind_worker_listener` (`SO_REUSEPORT` on Linux) and `listener_for_next_worker` |
| Route table (`crates/runtime/src/http_server/router.rs`) | none | `Vec<Node>` with `u32` child ids; static children as a dense first-byte list beside their ids; per-node `[u32; 10]` route slots and a `u16` method bitset; text in one `Vec<u8>` |
| Parameter values | none | `SmallVec<[(u32, u32); 4]>` byte ranges of the path, decoded on `param()` |
| Response headers | none | `SmallVec<[(String, PhpString); 4]>`; moved into the wire response when the handler's object is not shared |
| `Context` (`crates/extension-api/src/context.rs`) | no way to report an error and go on | `report_unhandled(error)` and `exit_script()` |
