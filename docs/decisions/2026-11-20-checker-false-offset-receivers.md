# Check array offset reads when a call can return false

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Report an ordinary offset read on an array-or-false union from level 7.
A successful non-null offset test narrows the named container on its
continuing path. Quiet reads such as a coalesce can handle the false
branch themselves.
