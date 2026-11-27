# 2026-11-27 — Unified `phera serve` + `Phera\Http\Router` & Persistent VM Pool

## Context

The initial `Phera\Http\Server` prototype ran its own sockets and worker threads inside `crates/runtime`, inverting the layering between `phera-runtime` and `phera-server`, dropping `phera serve` features (`--compress`, `--health`, WebSockets, `--dev` reload, debugger, trusted proxies), adding a two-task `mpsc`/`oneshot` hop per request, and serializing all requests on a worker onto one VM (blocking the worker during async I/O).

## Decision

1. **One server engine (`phera serve`)**:
   - `crates/server` stays transport-only (no `phera-runtime` dependency) and `crates/runtime` has no `phera-server` dependency. `crates/cli/src/serve.rs` remains the single glue layer.
   - `Phera\Http` exposes `Router`, `Request`, `Response`, and a convenience `Phera\Http\serve($router, $addr, $workers)` CLI handoff that delegates straight into `crates/cli/src/serve.rs`.
   - Scripts end with `return $router;` under `phera serve app.php`, or mount ahead of static files and classic PHP via `[server] routes = "routes.php"` / `--routes`.
2. **Per-worker persistent VM pool (`idle: Vec<PooledVm>`) — amendment to 2026-11-20 snapshot-and-fork rule**:
   - Each worker boots the route script once, freezes a `Snapshot` (forking `RouterData` and its captured handler/`onStart` closures natively via `ClassHooks::fork` and `ActiveRouterSlot`), and maintains a local `idle: Vec<PooledVm>` stack capped at `active_requests` (default 32).
   - Fast sync routes finish in one poll and reuse a single warm VM forever with zero forks and zero channel hops.
   - Concurrent async I/O routes auto-fork additional persistent instances from the route snapshot (running `onStart($worker)` once per instance) so requests multiplex M:N on the worker's `LocalSet`.
   - Framework front controllers (Laravel, Symfony) keep fork-per-request isolation; hybrid mode lets hot API endpoints run on the persistent router beside them.
3. **Phase 1 transport fast paths in `crates/server`**:
   - Poll `handler.handle(request)` once before arming `tokio::time::timeout` and `ResponseTask` so sync handlers skip timer-wheel registration/cancellation completely.
   - Pre-size header vectors and fast-path common static `HeaderName`s in `connection.rs`.
