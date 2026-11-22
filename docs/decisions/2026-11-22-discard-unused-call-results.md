# Discard unused call results

Date: 2026-11-22

## Decision

Mark calls such as `advance();` and `for (; running(); advance()) {}` whose enclosing expression does not use their result. The final condition in a `for` still reads its result. An explicit `(void)` continues to suppress PHP's `NoDiscard` diagnostic.

A direct native call can omit its result slot when the compiler proves the slot is empty before the call. The register number reserved for this purpose is outside every callable frame. Binding and continuations still use their ordinary paths, and completion drops the result instead of publishing it. The ownership pass can remove the resulting empty-slot cleanup on ordinary and exceptional paths.

| Structure | Was | Is |
| --- | --- | --- |
| Discard marking | Scanned the end of a standalone expression statement | Shared by standalone expressions and the unused parts of a `for` |
| Direct native result operand | Always a caller register | A caller register or the reserved no-destination value |
| Cleanup of a direct native result | Release after writing the result or unwinding the call | The ownership pass can omit cleanup of the proven-empty slot |

## Turned down

- Replacing an occupied destination with the no-destination value. Overwriting the old contents can release an object or change copy on write.
- Inferring that a deeply nested call is unused from the enclosing statement alone. Its result can be an argument or a condition.
- Skipping argument binding or a suspended continuation. A discarded value can still require PHP callbacks and can throw.
