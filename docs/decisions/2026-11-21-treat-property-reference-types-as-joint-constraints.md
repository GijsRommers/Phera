# Treat property reference types as joint constraints

- Date: 2026-11-21
- Area: Typed property references
- Decided by: Design

## Decision

Check each typed property against the original assigned value. Commit the
new value only if all holders agree on its conversion. When binding a new
property, a reference cell can change type only if it has no existing
typed property holders.

Reference acquisition checks whether a non-nullable property is
initialized before creating its shared cell. Property and reference
errors print the class name and value type PHP exposes to the script.

## Why

A reference has one value shared by all aliases. Converting it in
property order can hide a conflict and leave a previous holder with an
invalid type. The existing holder index already identifies the relevant
declarations.

## Turned down

Changing property storage or introducing a new representation for typed
reference cells.
