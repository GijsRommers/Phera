# Report unmatched PHPStan inline ignores by default

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Apply PHPStan's default unmatched-inline-ignore reporting when a project
has no checker configuration. Keep the existing configuration override.

## Why

An ignore can outlive a rule or refer to a rule outside the selected level.
PHPStan reports it even in a project with no configuration file.

## Turned down

Requiring a local setting solely to get PHPStan's core default.
