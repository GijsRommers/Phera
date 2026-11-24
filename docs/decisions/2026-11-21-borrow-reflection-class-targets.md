# Borrow reflection class targets

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Borrow a class name from an object or a valid UTF-8 PHP string while
resolving a reflection target. Allocate when lossy conversion requires
new bytes or when an owned reflection method target needs the name.

## Why

The runtime lookup already takes a borrowed string. Enum reflectors are
on the warmed Doctrine hydration route, where the same lookup visited
the string allocated by the extension and immediately released it.

## Turned down

A separate table of resolved reflection targets would retain results
across calls and overlap class resolution. Existing lookup and autoload
behavior stays in the engine.
