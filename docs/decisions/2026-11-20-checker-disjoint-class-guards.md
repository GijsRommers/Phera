# Exclude unrelated concrete classes on an instanceof path

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Drop an incompatible class arm when its ancestry and the tested class's
ancestry are both known. Keep intersection narrowing for interfaces.

## Why

Objects can implement several interfaces but cannot extend unrelated
classes. Leaving an impossible arm in the refined type can hide a
property declaration needed for a later typed write.
