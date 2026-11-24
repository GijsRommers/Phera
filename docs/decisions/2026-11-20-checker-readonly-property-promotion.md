# Use promoted readonly properties in redundant coalesce checks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

A promoted readonly property is set by the constructor. When the project
enables the unnecessary coalesce rule, it can report `$object->field ?? null`
for such a property on a known non-null receiver. A plain property with
no default still needs initialization evidence.
