# Keep implicit mixed element types after a native array write

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

When a mixed value is assigned to a native array property, successful
storage proves that the outer value is an array. At level 10 the element
type can still be implicit mixed.
