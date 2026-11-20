# Measure the large Symfony reference against core PHPStan rules

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Measure the 2,215-file Symfony reference separately from the five requested
targets. Use core PHPStan 2.3.7 at each level and load Composer symbols.

## Why

The reference application supplies a larger vendor index and application
workload. Its own PHPStan configuration requires a generated Symfony
container XML that the supplied checkout lacks.

## Turned down

Replacing the requested Symfony sample with the reference application, or
treating the core-rule measurement as a measurement with framework plugins.
