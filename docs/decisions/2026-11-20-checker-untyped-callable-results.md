# Treat undeclared callable results as implicit mixed

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Known `Closure` and `callable` types without a return signature produce
implicit mixed when invoked at level 10. A declared signature supplies
its return type. When all alternatives are callable, join their returns.
Unresolved invocations keep an unknown result.
