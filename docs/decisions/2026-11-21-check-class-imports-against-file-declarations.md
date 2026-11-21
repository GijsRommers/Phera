# Check class imports against file declarations

- Date: 2026-11-21
- Area: Class declarations
- Decided by: Design

## Decision

Use the class-name rules for both declarations and import aliases. A
class import checks declarations already compiled in that file and
namespace. Importing the declaration's own fully qualified name is
allowed. Compiling an already accepted underscore-named class or
interface emits PHP's deprecation.

## Why

An import changes how names in the file are resolved. A declaration
that appears before it reserves the class name even if execution has
yet to reach it. Runtime ancestry checks separately depend on whether
the class is available.

## Turned down

Checking the runtime class table for source-file import conflicts.
