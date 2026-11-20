# Use omitted boolean defaults in conditional return types

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Save literal boolean parameter defaults in the signature and substitute
them into a PHPDoc conditional return type when the caller omits that
parameter. Use the existing treatment of omitted null defaults.

## Why

`($asStream is true ? resource : string)` resolves to string for a call
that omits a parameter whose default is false.

## Turned down

Assuming a default is used when an unpacked argument could supply it,
or evaluating arbitrary default expressions.
