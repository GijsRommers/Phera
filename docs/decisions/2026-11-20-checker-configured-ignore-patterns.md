# Match configured PHPStan ignores with PCRE

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Honor message patterns, identifiers, paths, and counts in
`ignoreErrors`. Use the workspace's `pcre2` crate, maintained by
Andrew Gallant, for PHPStan's PCRE patterns. Keep the existing
baseline loader for generated PHPStan baselines.

## Why

Configured patterns use PCRE syntax such as lookahead. A literal
matcher adequate for generated baselines cannot match these patterns.

## Turned down

Executing PHP to match patterns or suppressing findings when a pattern
cannot be compiled.
