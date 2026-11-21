# Initialize static properties when taking a reference

- Date: 2026-11-21
- Area: Typed properties
- Decided by: Design

## Decision

When PHP acquires a reference to an uninitialized static property,
reject a type that excludes null. A nullable property becomes
initialized before the existing reference cell is shared.

## Why

Creating an alias is a PHP write when the property has no value.
An internal static cell can contain null while PHP still sees the
property as uninitialized. The alias operation is where these states
resolve.

## Turned down

Initializing a property whenever its internal cell is first allocated.
Reading a property must preserve its initialization state.
