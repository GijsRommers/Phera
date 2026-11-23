# Global symbols follow the active file scope

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

A global symbol lives in a named register of the active file-scope frame when
one exists. The request table owns names without a register. A file-scope
include moves the symbols its window represents into that window and returns
them to the caller's scope on exit. An include inside a function still uses
that function's locals.

## Why

Ordinary compiled variables can stay on the register path. Explicit global
declarations, dynamic scope operations and `$GLOBALS` use the same binding
even when a name was introduced by another unit.

## Turned down

Mirroring every assignment between separate register and global table bindings.
