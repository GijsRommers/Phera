# Debugging worker boot is explicit

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

`phera serve --debug --dev --debug-boot` connects each worker's boot VM to
the IDE, including a boot repeated after source changes. The boot
connection ends at `Phera\snapshot()` or when the script ends. Every
debugged request receives its own connection after it forks. A snapshot
contains PHP state and no debugger transport.

## Why

Breakpoints can stop boot code only while that code runs. A developer can
choose to stop startup without making ordinary request debugging depend on
an IDE connection during startup.

## Turned down

Starting a debugging session for every boot whenever request debugging
is enabled.
