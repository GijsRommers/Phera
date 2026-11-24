# Check nullable offsets in arrays indexed by class strings

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

An array indexed by class strings has no corresponding offset for null.
From level 7, report reads using a possibly null string key before a guard
establishes that the offset exists.
