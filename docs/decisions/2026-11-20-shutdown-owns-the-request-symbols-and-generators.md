# Park request symbols in PHP order

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Reserve each file-scope symbol's position when its scope is attached. Its
value can move between a register and the request table without changing
that position. Publish the bindings before releasing the file-scope stack.
Shutdown functions and Batch 14's destructor phases use the same ordered
symbols.

## Why

PHP attaches compiled names to an ordered symbol table before executing
file-level code. A named assignment declares its target before compiling
its source, so a chain of assignments reserves symbols from left to right.
A fiber switch or include moves the values, and a shutdown callback can
replace them. Teardown still visits sole-owned objects in reverse symbol
order and runs the remaining object and generator destructors by ID.

## Turned down

Retaining a second teardown pass for generators or rebuilding the symbol
order when a file-scope frame is parked.
