# Factory classes guard script allocation

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

A native class can attach a class-specific allocation error to its hooks.
`new` and reflective construction check it before allocating an instance.
A native factory can still allocate the object to attach its native state.

## Why

PHP can refuse script allocation without declaring a constructor.
`Directory` uses the same allocation path for literal and computed class
names, and `dir()` remains its factory.

## Turned down

Registering a synthetic private or public constructor to report the error.
