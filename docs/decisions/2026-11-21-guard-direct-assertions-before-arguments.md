# Guard direct assertions before arguments

- Date: 2026-11-21
- Area: PHP behavior
- Decided by: Design

## Decision

Lower a resolved direct call to the built-in assert with a guard before
the argument sends. The guard reads the request's effective assertion
setting. Disabled direct assertions yield true. Indirect calls retain
ordinary argument evaluation. Enable startup mode -1 and reject
runtime transitions between negative and nonnegative modes.

## Why

PHP's direct syntax skips side effects in both disabled modes. Code
compiled once can still observe ini_set changes between 0 and 1.
The compiler already owns the source description used by a failing
direct assertion, including calls resolved through namespace imports.

## Turned down

Checking the setting only inside the extension lets argument side
effects happen. Specializing a shared compiled unit at startup would
need to include the setting in how that unit is reused.
