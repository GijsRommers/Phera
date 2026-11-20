# Validate calendar components before applying them

- Date: 2026-11-20
- Area: intl error compatibility
- Decided by: Design

## Decision

Check PHP's signed 32-bit range for numeric calendar components before
changing calendar state. Report the PHP parameter name and position for
each method or procedural signature. Validate wall-time options against
the constraints of each setter.

## Why

ICU4C accepts signed 32-bit calendar inputs. PHP detects larger values at
the call boundary and raises an argument-specific ValueError. Repeated and
skipped wall-time options admit different sets of values.

## Turned down

Letting civil-date normalization clamp overflow, and sharing the valid
skipped options with the repeated option setter.
