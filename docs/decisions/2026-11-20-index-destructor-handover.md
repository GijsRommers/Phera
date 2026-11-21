# Index destructor handover by object id

- Date: 2026-11-20
- Area: Value layout
- Decided by: Design

Use a vector of optional weak handles and original identities for objects
handed over for a destructor. Object ids are consecutive at allocation and
the pool reuses them. The first handover at a higher id grows the vector,
and releasing a handed-over object clears its slot.

A weak reference can follow its object into the destructor and through
resurrection. The original identity check still prevents references from
following a later object that reused the same id.
