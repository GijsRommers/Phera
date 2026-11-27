# Request paths are looked up on the worker thread

- Date: 2026-11-27
- Area: Server and requests
- Decided by: Design

## Decision

`phera serve` finds what a request path names (`Site::resolve`: one `stat`
for a path that is no file, a `stat` and a `realpath` for one that is) on the
worker thread that handles the request, as nginx does in its event loop. Only
answering a static file, which opens and reads it, still goes to Tokio's
blocking pool.

## Why

Every request used to hand `Site::resolve` to `tokio::task::spawn_blocking`.
That hop costs more than the lookup: the pool's queue is a mutex and a
condition variable shared by the worker and the pool's threads, the result
comes back through the worker runtime's remote queue (another mutex) and a
reactor wake-up, and the request, its path and the response were allocated on
one thread and freed on another. Under load the workers and pool threads
contend on those locks on every request. It was the only lock that every
`/plaintext` request took across threads, so it is the likely source of the
contended-mutex symbol at the top of Hades' `perf record` (about 9 to 11% of
the server's self time); that has to be confirmed there, `perf` does not run
on the Mac. The lookup itself is a cached metadata call of about
a microsecond.

Measured on the Mac (2 workers, `oha -c 64`, lowest of three rounds,
instructions and cycles of the whole server process per request):

| Route | Instructions | Cycles | CPU per request | Requests/s |
| --- | --- | --- | --- | --- |
| `/plaintext` before | 316.5k | 141.4k | 38.8 µs | 71.9k |
| `/plaintext` after | 275.8k | 86.7k | 22.1 µs | 89.9k |
| `/json` before | 326.0k | 143.4k | 39.3 µs | 70.3k |
| `/json` after | 285.8k | 89.9k | 23.3 µs | 85.1k |

## Turned down

- **A per-worker cache of resolved paths.** It would hide the hop for repeated
  paths only, and must be invalidated when files appear or go; the hop itself
  was the cost.
- **Keeping the lookup off the worker for slow file systems.** A document root
  on a network file system can stall a `stat`; such a site should serve its
  files from local disk. PHP-FPM behind nginx makes the same choice.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Path lookup per request | `spawn_blocking` (pool mutex, condvar, two thread switches, cross-thread frees) | direct call on the worker thread |
| Static file answer | inside the same `spawn_blocking` | its own `spawn_blocking`, static files only |
