# Infer templates independently of their null alternative

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When matching an argument to a parameter with separate template and null
alternatives, infer the template from the non-null part of the argument.

## Why

A method can accept `T|null` and return `T`, for example by throwing
when the argument is null. PHPStan uses the argument's null alternative
only for the explicit null arm of the parameter.
