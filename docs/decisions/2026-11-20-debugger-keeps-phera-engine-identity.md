# The debugger identifies itself as Phera

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

DBGp init reports Phera and its version. Clients can discover supported
features with DBGp commands. PHP code can read the effective
`xdebug.*` startup values via `ini_get()` and request a stop via
`xdebug_break()`. The latter returns false when there is no attachment.

`extension_loaded('xdebug')` and `phpversion('xdebug')` return false.
PhpStorm's setup validator may report that the Xdebug extension is absent.
Starting the IDE listener and stopping at a source breakpoint exercises
the Phera debugger directly.

## Why

Frameworks use the extension check for more than step debugging, including
coverage, profiling, and tracing. Phera implements the step debugger and
chooses the DBGp features it can provide.

## Turned down

Reporting an installed version of Xdebug or implementing its profiler,
tracing, coverage, or develop-mode output.
