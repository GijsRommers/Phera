# Constants and globals resolve in the interpreter loop

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

## Decision

Three instructions that left the interpreter loop at every run now run in it
(`Machine::further_step`), from flat tables of the file's cache
(`UnitCache`), one cell per symbol or name:

- **`FetchConstant`**: the first read of a global constant that is a scalar or
  an interned string, without a deprecation, is kept in the file's
  `global_constants` table with the constant table's epoch. Every `define()`
  or `const` gives the table a new epoch, from one process-wide counter, so
  a kept value holds exactly while the epoch is the same. Arrays, enum cases
  and deprecated constants go through the VM as before.
- **`BindGlobal`** (`global $x`): the first binding remembers where the
  variable lives, in the file's `global_bindings` table: a declared register
  of the file-scope code, or an entry of the request's globals table by
  index. Later bindings check that it is still there and already a PHP
  reference, and copy the reference. A variable made by name (`$$name`) is
  not remembered.
- **`Assign`** to a local that is a PHP reference, of a scalar or string,
  over a scalar or string: written into the shared cell, when the loop is
  not guarded, no reference of the request is bound to a typed property, and
  the frame has no side table (no `foreach` that could be iterating it).

## Why

`gen_random` of `Zend/bench.php` runs `global $LAST`, three constant reads
and an assignment through the reference per call: each left the loop, and
the VM looked the constant up by hashing the site, and the global by walking
the frames and comparing the names of the file-scope code's variables.

## Turned down

- **A `HashMap` from name to value in the loop.** The names are already
  dense indexes of the file (`SymbolId`, `NameId`).
- **Invalidating kept constants on each definition.** An epoch number costs
  one comparison and needs no walk over every file's cache.
