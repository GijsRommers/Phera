# Check declaration type completeness against inherited contracts

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Level 6 checks declaration types, iterable values, and missing arguments to
generic classes. Use parameter, return, and property PHPDoc, including the
types of matching inherited declarations and promoted properties.

## Why

An implementation can inherit its type contract from an interface or base
class. A rule that looks only for local type syntax would ask for redundant
PHPDoc and can miss a value type that is still missing.

## Turned down

Requiring implementations to restate every inherited type.
