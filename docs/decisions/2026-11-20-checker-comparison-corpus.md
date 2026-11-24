# Compare the checker with PHPStan on fixtures and Composer projects

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Record PHPStan 2.3.7 diagnostics in Rust regression fixtures. Compare releases
with the same PHPStan on Symfony and Laravel application code, Symfony String,
Doctrine Collections, and PHP-Parser. Match findings by file, line, and
identifier, counting repeated findings at the same location. Record messages
in the fixtures because PHPStan baseline entries can match message text.

## Why

A small fixture isolates the rule under test. Composer projects also exercise
symbol discovery, inherited types, and control flow across real code. The
package sources exercise higher levels even when the application is small.

## Turned down

Putting the measuring script and its Composer installation in the repository.
Release comparisons can run separately from the checked-in Rust fixtures.
