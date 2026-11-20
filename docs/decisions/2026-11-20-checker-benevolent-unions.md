# Preserve PHPStan's benevolent unions

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use a distinct type for PHPStan's benevolent unions, including
unspecified PHPDoc array keys and the result of regex replacement
on an unknown subject. An ordinary union still checks all applicable
alternatives at the stricter levels.

## Why

PHPStan treats an unspecified array key as a value that can be used
as an integer or a string. The same policy does not apply to an
explicit `int|string` declaration.

## Turned down

Making all unions permissive or replacing a benevolent union with
mixed and losing its available type information.
