# Native parameter unions use one declaration

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

A native parameter can declare a union of named classes and interfaces,
with an optional string arm. Argument validation, diagnostic text and
reflection use this declaration. Nullability remains part of the parameter.

## Why

An object's ancestry decides whether it matches a named member. A matching
object stays an object even if the union also permits string coercion.
The scalar-only unions keep their PHP coercion rules.

## Turned down

Adding another parameter-type variant for each fixed pair of classes.
