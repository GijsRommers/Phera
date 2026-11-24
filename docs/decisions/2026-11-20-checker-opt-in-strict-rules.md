# Enable strict rules from the PHPStan include

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Make the strict rules an opt-in group. Recognize the installed
PHPStan strict-rules include and allow an explicit Phera setting and
command-line flags to override it.

## Why

For example, reusing a defined variable as a foreach key or value
is reported when this package is enabled, even at level 0.

## Turned down

Applying strict-rules diagnostics to a project using core rules.
