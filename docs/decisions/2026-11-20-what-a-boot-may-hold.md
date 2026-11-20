# What a boot may hold at Phera\snapshot()

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Claude (follow-up to the owner's "never disabled")

## Decision

Every kind of state a real boot was found to hold forks, and so do the
kinds a boot plausibly holds. Each fork gets its own:

| State | In each fork |
| --- | --- |
| mysqli, PDO, pgsql connection | unconnected copy; borrows from the pool on first use and replays the boot's session statements (`2026-11-20-boot-connections-reconnect-in-each-fork.md`) |
| stored `mysqli_result` | copy of its rows, same position |
| Redis connection | copy without a socket; borrows one on first use, `AUTH` and `SELECT` again |
| file opened with `fopen()` | opened again by its resolved path, same mode, never created or truncated, at the boot's position |
| `tmpfile()`, `php://temp` past its limit | contents copied into a new temporary file |
| DOM document and its nodes | one copy of the document per fork, shared by that fork's node objects (`Fork::shared`) |
| `HashContext` | copy, continues the boot's input |
| `CurlHandle`, `CurlMultiHandle` (no transfers), `CurlShareHandle` | copies; callbacks and streams in options copied; a share copied once and attached to the fork's handles; a persistent share keeps the worker's DNS cache |
| `DeflateContext`, `InflateContext` not used yet | made again with the same options |
| `Generator` in any state | its register window and pending calls copied; resumes where the boot left it; `finally` runs at each request's end |
| `Fiber` not started, or finished | copy; starts in the fork, or keeps its result |

Still refused, because the state cannot be duplicated: an open transaction,
table locks, temporary tables and other session state a replay cannot make
again; a file under `flock()`; a suspended `Fiber` (its stack is a VM of its
own); a generator suspended inside a `finally` block or delegating to an
iterator; a pipe to a process; a directory handle; an open gzip file; a
compression context data already went through; prepared statements
(`PDOStatement`, `mysqli_stmt`) and a result still streaming; an `SQLite3`
object. Each refusal names the class and where it is held, and the worker
answers from the top, loudly (`2026-11-20-snapshots-are-the-only-mode.md`).

## Why

Every refusal sends a script's requests to the from-the-top path. The real
boots surveyed (Laravel production, Symfony with Doctrine in prod and dev,
WordPress) now snapshot with no refusal.

## Turned down

- Recording all input of a compression context to replay it: memory for
  every context, for a case no boot was seen to have.
- Duplicating a running process or a fiber's stack: there is no second copy
  of either.
