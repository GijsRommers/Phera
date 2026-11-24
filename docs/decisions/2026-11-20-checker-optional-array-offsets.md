# Check optional offsets in closed array shapes

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

From level 7, a literal-key read can be reported when its field is
optional or present in only some members of a closed shape union.
An `isset` or `array_key_exists` guard establishes the field on its
true path. `array_key_exists` permits a null value.
