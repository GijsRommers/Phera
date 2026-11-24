# Check strings interpolated from mixed values

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

An explicit mixed return annotation on a prototype applies to an
override that leaves its return unspecified. Check interpolation of
a named variable from that result at level 9. At level 10 the same
rule applies to implicit mixed.
