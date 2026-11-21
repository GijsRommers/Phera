# Bound JSON ancestor searches

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Scan the nearest 32 open parents directly. Index older active identities in
an ordered set as the cursor descends and remove them as it climbs.

## Why

A flat value still uses one inline work entry. An unbounded ancestry scan
made encoding cost grow quadratically with nesting depth. Spilling older
identities keeps the same recursion detection for shared containers and
bounds the work per encoded member.

## Turned down

A hash table for all active containers. Shallow JSON needs only the work
stack it already has.
