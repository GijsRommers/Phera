# Snapshots are the only mode and are never disabled

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Owner

## Decision

The owner: "Snapshot should be default and never be disabled pls". `phera
serve` has no option, INI setting or environment variable that turns
forking off; `--no-snapshot` is removed. A state that cannot be kept at
`Phera\snapshot()` is a defect, not a mode: the worker still answers its
requests correctly by running the script from the top, and says so loudly,
one line per occurrence in the server log naming the class and where the
value is held (`DEFECT: ... cannot be snapshotted`), and a process-wide
count in the `--health` answer's `Phera-From-The-Top` header. Every
refusal a real boot meets is to be made forkable. `--dev` keeps using
snapshots. (`crates/runtime/src/snapshots.rs`, `crates/cli/src/serve.rs`)

## Why

Requests from the top cost five times a fork; a silent fallback would hide
that cost behind correct answers.

## Turned down

A hidden option for tests: the tests that need a run from the top use the
runtime's own API (`Runtime::handle_request` and a `WebWorker` that was not
booted run every request from the top), which no user can reach from the
command line.
