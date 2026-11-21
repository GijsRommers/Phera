# Allow array promotion in iterable properties

- Date: 2026-11-21
- Area: Property initialization
- Decided by: Design

## Decision

Check whether a declaration accepts arrays before promoting an empty
property to an array. The existing iterable mask permits the promotion.

When indexing an empty static property, validate its declared type
before promotion, and mark the slot initialized after the write.
Promotion through an alias checks every typed holder before creating
the array. The checks run only on a null or false value that is about
to become an array.

## Why

An array satisfies an iterable property type. Keeping this check at the
point of promotion preserves the uninitialized slot when its declaration
forbids arrays.

## Turned down

Initializing the slot before checking the declaration.
