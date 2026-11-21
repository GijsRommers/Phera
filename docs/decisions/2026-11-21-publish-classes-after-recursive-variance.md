# Publish classes after recursive variance checks

- Date: 2026-11-21
- Area: Class linking
- Decided by: Design

## Decision

When a method contract needs autoloading, keep the class at its reserved
index outside the published name table. Its shape supplies ancestry to
other declarations. A child that uses the shape waits for its pending
parents and interfaces before publication. Use an explicit work list
and mark each requested type before running its autoloader.

An executing declaration whose parent or interface is still loading can
supply its declared ancestry. A conditional branch that has not run
supplies no such information. If another class linked against the
executing declaration and its autoloader then throws, report a fatal
inheritance error with the exception trace.

## Why

A class and a type in its method contract may require one another.
For example, an overriding method can return a subclass whose parent
is the class being linked. Ordinary PHP lookups only see completed
declarations. The loader that defines the subclass can run more PHP
as soon as both declarations pass their checks.

Classes linked in this context stay request-local. Their dependencies
include declarations that were visible only to the current linker.

## Turned down

Publishing a half-linked class through ordinary lookup. An autoloader
could use the class while its contracts were unverified. Looking up
all conditional source declarations would skip observable autoload
callbacks.
