# Check nullsafe calls on unions of known objects

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Report a nullsafe call when every known alternative is an object.
If the evaluated call also has a known non-null result, report a
coalesce that uses that result as its left operand.
