# Compare all values in an integer range at level 7

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

A return or argument type such as `-1|positive-int` excludes some values
of `int`. From level 7, compare the given interval with the union of
intervals in the contract. Keep the existing checks for coercion and
contracts that contain other kinds of values.
