# Each thread mirrors the interned strings it uses

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

## Decision

`InternedStr::try_intern` looks in a thread-local table of the interned
strings that thread has used (pointers into the process table) before it
takes the process table's lock. Interned strings are immortal and never
change, so the mirror never goes stale. A miss takes the lock once and adds
the string to both.

## Why

The process table is one mutex shared by every worker thread. Strings are
interned at run time too: PDO interns the column names of each statement's
result, a unit's literal and class-name values are made when a request loads
it, PHP constants of native extensions on registration. With several workers
those calls met on the lock. The mirror costs 8 bytes per string a thread
uses, at most about 1 MiB per thread at the table's cap.

## Turned down

- **A lock-free shared table.** A concurrent hash map is either unsafe code or
  a dependency with its own locks; per-thread mirrors of immortal data need
  neither.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| `try_intern` of a known string | process mutex, probe | thread-local probe, no lock |
| New string | process mutex, hashed twice | process mutex, hashed once, added to the thread's mirror |
