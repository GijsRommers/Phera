# Discard unused call results

Date: 2026-11-22

## Decision

Mark calls whose enclosing expression does not use their result, as in `advance();` and `for (; running(); advance()) {}`. An explicit `(void)` continues to suppress PHP's `NoDiscard` diagnostic.

Use a reserved call-destination operand when the compiler proves the old slot empty or scalar. Each existing dispatch arm forwards its destination. The frame records that the return has no slot, and native calls retain their binding and continuation paths. The ownership pass can then omit redundant normal and exceptional cleanup. If the old slot may hold an owner, the call still publishes to it and the compiler keeps the release.

For a discarded temporary return, such as `return new Result;`, release the temporary before the callee's local variables. A returned local stays in local cleanup order. Construction releases the receiver after constructor-local cleanup, and a discarded generator still creates and releases its activation.

| Structure | Was | Is |
| --- | --- | --- |
| Discard marking | Scanned the end of a standalone expression statement | Shared by standalone expressions and the unused parts of a `for` |
| Call result operand | Always identified a caller register | Identifies a caller register or carries the reserved no-destination value |
| Return from a statement call | Published a result or looked ahead for its explicit release | Uses the frame's reserved destination when the compiler proved the slot redundant |
| Cleanup after a call | Cleared the result slot on ordinary and exceptional paths | The ownership pass can omit releases when the call writes no result |

## Turned down

- Setting a discard flag while packing every frame. The direct-entry path gained a comparison and flag selection even for used results.
- Replacing a destination that may hold an owner. Overwriting its old contents can release an object or change copy on write.
- Inferring that a nested call is unused from its enclosing statement. The result can feed an argument or a condition.
- Skipping argument binding or a continuation. A call used for its effects can still invoke a callback or throw.
