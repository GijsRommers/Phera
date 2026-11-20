# Native deprecation notices precede argument binding

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the PHP stub catalog for the retained callables' deprecation text.
The deprecated function entry and method preparation hooks emit the
notice before checking arguments. Reflection uses the same catalog for
the installed retained callables.

## Why

PHP emits the same notice for a retained native callable even when its
argument binder subsequently throws. The stub message and PHP version
belong in one place.

## Turned down

Emitting the notice only inside a successfully bound function or method.
