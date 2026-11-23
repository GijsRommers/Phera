# Check one flag before include wrapper lookup

- Date: 2026-11-20
- Area: Runtime
- Decided by: Design

## Decision

The request's include state carries one flag for a wrapper table that can
intercept or disable an include. Wrapper registration changes the flag.
The snapshot copies it with the rest of the include state.

## Why

Most requests use the filesystem for every include. The flag leaves those
includes on the existing loader path and keeps wrapper resolution off it.

## Turned down

Acquiring the code loader and asking the wrapper table about every include.
