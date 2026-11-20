# Keep proven mixed provenance at a branch join

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

When a variable is defined on all paths and one path produces mixed
while another lacks type evidence, retain implicit mixed. The known
path already establishes a value that level 10 has to check.
