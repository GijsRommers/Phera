# Compound concatenation of an array element or property that holds an object converts it through PHP code

- Date: 2026-11-21
- Area: Operators
- Decided by: Design

## Decision

`$a['k'] .= 'x'` and `$o->p .= 'x'` with an object on either side go to the asynchronous handler, as a `Binary` concatenation of an object already did. That handler can call `__toString`.

## Why

The general path computed the result with the value layer's operator, which cannot run PHP code, and threw "Object of class S could not be converted to string" for a `Stringable` object. PHP 8.5 gives the concatenated string. A local variable (`$v .= 'x'`) was already right.
