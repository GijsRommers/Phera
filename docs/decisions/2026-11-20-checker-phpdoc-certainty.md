# Confirm constant conditions with native types when requested

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When `treatPhpDocTypesAsCertain` is disabled and potential constant
condition findings exist, confirm those findings using a pass with
native types. Use PHPDoc as usual for declaration and generic checks.

## Why

A PHPDoc type can be valuable for checking a public contract without
being a guarantee that a condition is constant at runtime. A native
pass can confirm full control-flow facts without tracking a second
type on every expression in the default configuration.

## Turned down

Disabling PHPDoc analysis for every rule or running a second pass
when PHPDoc is considered certain.
