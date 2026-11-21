# Reflect registered constant defaults

- Date: 2026-11-21
- Area: Native parameter reflection
- Decided by: Design

## Decision

When a native registration supplies a named constant default and the
stub names the same constant, reflection uses the registered value.
The stub still supplies the signature, default name and display text.

## Why

The generated parameter table records values from the build that produced
it. A platform constant such as `SOMAXCONN` can have a different value
in the current build. A reflected default must agree with the value the
function uses when the caller omits the argument.
