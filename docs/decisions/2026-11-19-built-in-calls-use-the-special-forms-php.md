# Built-in calls use the special forms PHP's own compiler has (`strlen`, `count`, type checks, `in_array` with a literal, `sprintf`, constant folding of pure functions)

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

Built-in calls use the special forms PHP's own compiler has (`strlen`, `count`, type checks, `in_array` with a literal, `sprintf`, constant folding of pure functions). A shortcut tests properties of the call and never the spelling of a script.

## Why

They are observable in PHP only through speed, and each has a fixture proving equal behaviour, traces included.

## Turned down

Shortcuts tuned to benchmark scripts.
