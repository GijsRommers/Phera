# Check fallthrough against inherited PHPDoc return contracts

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Check a method with no native return type against its own documented
return type or its prototype's return type. From level 2, nullable PHPDoc
still requires a return on each continuing path. Import
`checkPhpDocMissingReturn` and allow a `[check]` override.

## Why

A nullable contract permits returning null. PHPStan distinguishes that
result from falling through the body without a return statement.

## Turned down

Applying a prototype's PHPDoc return type to call-site and explicit
return checks as part of this change.
