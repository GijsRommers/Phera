# Link a method site to its record and single local

- Date: 2026-11-23
- Area: Speed and method
- Decided by: Design

## Decision

The compiler assigns each method site a record index across the file. Resolving the method checks its signature once. For a plain method with one untyped by-value parameter and a single local send, the existing record holds the local register, function and register count. A hit checks the receiver class index, effective scope, stack limit and current value before it copies the argument into the callee and retains the receiver as `$this`. An undefined local or a reference goes through the original instructions at its send position.

## Why

The call-path ledger attributed 64 excess instructions to the site lookup and 54 to sending the argument. Its estimate for this change was 65–90 retired instructions per method iteration. In the reviewer's settled Symfony profile, the loop entered the method-site arm 40,448 times, costing 23.54M instructions on the profiled revision. That count includes native and general entries.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Site record address | Per-file table of function starts plus function-local site index | File-wide index on the compiled descriptor |
| Class and scope guard | Class-handle address and effective scope index | Receiver class index and effective scope index |
| One-local PHP entry | Walk the argument descriptors and check the parameter on every hit | Record the eligible source and frame width when the site resolves |
| Receiver and fallback | Named receiver retained, taken temporary transferred, ordinary sends available | Same ownership and written-out sends, with specialization limited to retained receivers |

## Turned down

A reference can appear in a local between visits to the site. A rebound closure can also exercise the same site with different private visibility. Both guards must run before the window grows.
