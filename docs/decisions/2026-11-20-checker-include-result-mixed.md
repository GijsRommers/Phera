# Treat include results as implicit mixed

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

At level 10, check operations on the result of `include` and `require`
as operations on implicit mixed. The checker does not follow the result
through the included PHP file. After assignment to a native array
property, a successful write still leaves an array in the property.
