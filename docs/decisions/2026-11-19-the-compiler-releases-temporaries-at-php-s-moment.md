# The compiler releases temporaries at PHP's moment

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

The compiler clears a call's operands right after the call,
and the temporaries of `if`/`while`/`do`/`for` conditions and `foreach`
subjects on each way out, on top of the statement-end clears of #39. A
register that provably holds a scalar, or a copy of a variable the
expression does not assign, is not cleared; a condition starts with clean
registers. (`crates/compiler/src/expressions/operand_release.rs`)

## Why

PHP frees a call's temporary arguments when the call returns, so a
destructor runs before the result is used (Symfony's service configurators
register themselves in `__destruct` at the end of a fluent chain). Clearing
copies of variables cost 5.9 percent on bench-object-sort and freed nothing.

## Turned down

Reference-counting temporaries at run time (cost on every
instruction); clearing at statement ends only (too late for conditions,
which enclose their whole body).
