# Learn unbounded class strings from class_exists

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

A successful `class_exists($name)` proves the type `class-string`.
Starting at level 7, a contract such as `class-string<Handler>` also
requires evidence that the class belongs to that family.
