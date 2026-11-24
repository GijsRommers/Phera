# Check invalid keys and missing offsets on known arrays

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

From level 3, check known PHP 8.5 array keys and report absent literal keys
on closed shapes. A float key on a list also reports a missing offset.
Conditions that prove the offset exists suppress the latter diagnostic.
An inferred shape in a level 3 loop may still reflect the initializer, so
wait for loop widening before using it to decide that an offset is absent.

## Why

The same access can have an invalid key and fail to address any element.
ArrayAccess objects and quiet reads have separate rules.

## Turned down

Assuming that arbitrary array keys or the dimensions of an open shape
are absent.
