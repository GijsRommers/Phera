# Check mixed values in compound assignment and increment

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Use the binary operation check for compound assignment, with PHPStan's
assignment identifier and operator text. Increment and decrement use
their own identifiers. These checks follow the explicit and implicit
mixed thresholds.
