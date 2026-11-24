# Check assignments replaced before their value is read

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

When the project enables the unused-variable feature, report a local
assignment that is followed by a proven overwrite before a read. Track
straight-line statements and assignments in a branch that precedes the
same unconditional overwrite. A by-value closure capture is local to
that invocation.
