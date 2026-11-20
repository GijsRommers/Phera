# Keep PHP file precedence in the run command

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Treat an existing file, a path, or a name ending in `.php` as a file when
passed to `phera run`. Resolve another name to a composer.json script if
the manifest declares it. Use `phera run-script` to select a script with an
ambiguous name.

## Why

Phera already uses `run` to execute PHP files and passes subsequent arguments
through unchanged. A project script alias should not intercept an existing
file command.

## Turned down

Giving composer.json script names priority over PHP file paths.
