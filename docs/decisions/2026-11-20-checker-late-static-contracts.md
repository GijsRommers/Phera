# Check late static access against the declaring class contract

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

At level 0, report `new.static` if an indexed nonfinal class has no
constructor contract that guarantees the creation call. Recognize
final constructors, constructor prototypes, and consistent-constructor
annotations. Check late-bound constant names on abstract classes, too.

## Why

Subclasses can change an unconstrained constructor. A constant reference
in the base needs a declared member. PHPStan checks both at level 0.

## Turned down

Treating the abstract base itself as the object created by `new static`.
