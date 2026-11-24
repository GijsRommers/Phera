# Validate concrete returns against unresolved output templates

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

At level 7 and above, a concrete class returned from a construction or
call cannot establish a caller-selected template of that class family.
Check the result directly and in array value types when both contracts
are known.
