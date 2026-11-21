# Apply strict types in the file prelude

- Date: 2026-11-20
- Area: Compiler
- Decided by: Design

## Decision

Compile leading `declare` statements as the file prelude. Only a
standalone `strict_types` directive in that prelude can set strictness.
Use the existing strictness field when compiling the file and its functions.

## Why

PHP rejects a late directive before any code executes, including a directive
inside a branch that never runs. The top-level statement list identifies
placement before the compiler visits a directive's body.

## Turned down

Inferring placement from the number of emitted instructions, which would
miss preceding declarations and other statements that emit no code.
