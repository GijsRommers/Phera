# Retain inherited method contracts

- Date: 2026-11-21
- Area: Class linking
- Decided by: Design

## Decision

Check that an override of a method with a declared return type keeps a
compatible return declaration, including when the ancestor declares
`mixed`. Keep an abstract ancestor's constructor signature as a contract
when an intermediate class supplies a concrete implementation.

## Why

PHP requires a value from a method whose return type is `mixed`. The
`void` type is a separate contract. A concrete constructor implements
its abstract ancestor's contract, which still applies to a later
override.

## Turned down

Changing which class declarations participate in a variance check on its
own. During autoload, a class can depend on a class whose method types
are still being checked. Resolving this requires the linker to track
classes temporarily available for inheritance and defer their outstanding
variance checks. Without that state, narrowing the lookup caused three
php-src autoload tests to fail.
