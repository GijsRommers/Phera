# Report a false boolean result when the operands contradict each other

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Keep the operand's own diagnostic and separately report a false logical
result when evaluating the tests together proves it. For example,
`!is_string($item) && !$item instanceof Item` cannot be true for
`Item|string`.
