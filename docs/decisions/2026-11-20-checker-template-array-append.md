# Check integer appends to arrays with a template key

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Appending to `array<K, V>` can add a key outside K even when the value
conforms to V. Check this write from level 7 when the class declares K
and the writable property and appended element establish the contract.
Inherit a documented parameter when an override declares native mixed
and supplies no parameter annotation of its own.
