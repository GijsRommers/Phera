# Design decisions

## 2026-11-19

### Decimal arithmetic uses integer coefficients

BCMath stores each value as a signed `num-bigint` coefficient and an explicit
decimal scale. It aligns coefficients for addition and comparison and uses
integer division to truncate toward zero. This preserves trailing zeroes in
`BcMath\Number` while supporting computations beyond machine integer precision.
Floating point would lose decimal digits. A general decimal crate would still
require separate rules for PHP's truncation, rounding modes, and result scales.

### Native binary operators run after scalar dispatch

Native classes can handle an operator with both original operands through
`ClassHooks::binary`, or decline and let the VM apply the usual PHP rules.
The VM checks for a native hook after the scalar fast path and uses its existing
slow instruction dispatch. This keeps operand order for noncommutative
operations and avoids hard-coding BCMath or GMP arithmetic in the VM.
