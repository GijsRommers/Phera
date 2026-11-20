# Match Composer's generated package file format

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use Composer's bootstrap templates and property order in the PHP generators.
Write installed metadata with Composer's field order and indentation. Keep
the byte comparison separate from the package-file and autoload probes.

## Why

Matching bytes makes the same installed tree useful to both tools and makes
differences visible when a class map, path expression or metadata field changes.
The differential probes still compare PHP behavior for projects with format
differences.

## Turned down

Starting Composer during install to write the generated files. Native
generation keeps locked installs independent of a Composer executable.
