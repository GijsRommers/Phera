# Code caches and snapshots are one design

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

The process cache compiles files (and, under their names, the PHP Phera
ships); a worker keeps resolved include spellings and its snapshots; each
request, fork or classic, gets a fresh request code cache as its loader.
One include policy, opcache's settings from the start-up INI (with `--dev`
checking on every request), decides when includes and a snapshot's files are
checked. A changed snapshot boots again in the background; production
requests keep forking the old one meanwhile, `--dev` requests run from the
top until the new one is ready. `--preload` fills the process cache before
the boots. (`crates/runtime/src/snapshots.rs`, `crates/runtime/src/lib.rs`,
`docs/architecture.md` 5.4)

## Why

A fork must not keep the boot's request loader: its start time would decide
the fork's include checks. Two policies for the same files would disagree
about when code changed. Under `--dev` a developer must never see the code
from before an edit, while in production a few hundred milliseconds of the
old code during a re-boot is what opcache does too.

## Turned down

A separate interval for snapshots (two settings for one question); blocking
requests on a re-boot in `--dev` (a slow boot would stall the browser).
