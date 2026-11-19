# XML recovery follows the Linux CI oracle and preserves the older macOS libxml 2.9.13 behavior on macOS

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

XML recovery follows the Linux CI oracle and preserves the older macOS libxml 2.9.13 behavior on macOS.

## Why

The oracles differ in trailing diagnostics, undeclared entity recovery, and whitespace in mixed content. Both use the same comparison inputs.

## Turned down

Changing or dropping the version-sensitive oracle cases.
