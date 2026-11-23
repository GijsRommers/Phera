# Functions and linked classes are numbered by the worker, and shared by its requests

- Date: 2026-11-20
- Area: Engine, classes and code
- Decided by: Claude (design), approved by Fable

## Decision

- The worker gives each file revision's functions their numbers the first time any of its requests (or its boot) loads that revision (`crates/vm/src/worker_code.rs`). A request's code table stays one flat vector indexed by function number, so a call is still one load; only loading a file asks the worker. The vector holds placeholders for files the request has not loaded: whether a function is declared is the request's function table's answer.
- Numbers are reused: code without a file (`eval`) and methods composed from traits lease numbers for one request (a snapshot's leases are shared with its forks and go back when the last is gone); a file revision nothing holds any more gives its numbers to the next revision.
- A class a request links becomes an entry of the worker's table at a stable position (`crates/vm/src/classes/worker_classes.rs`). A request holds which entries it has declared (`Vec<Option<Rc<Class>>>` by position) and its names; whether a class exists in a request is never answered from the worker table.
- A request reuses an entry made from the same declaration (file revision, class number in the file) when every class name the entry's linking looked up, found or not (parent, interfaces, traits, classes named in signature checks), is the same class or no class in that request. It then writes what linking records in the request (its methods' class scope, the facts of the trait methods composed into it) instead of linking. Otherwise it links and adds an entry.
- Static and constant slots are numbered by the worker; a request's tables grow to a slot when it first uses it.
- Linked per request, never shared: a class whose linking raised a diagnostic (its handler belongs to that request), enums (their cases are the request's), anonymous classes.
- Debug builds print what each fork copies, by structure (`PHERA_FORK_FOOTPRINT=1`).

## Why

A fork that declares a class the boot never touched linked it again in every request. The rule for sharing is a fact of the language (the same declaration with the same classes around it gives the same class), recorded with the entry when it is made, not a cache with a key looked up per request.

## Turned down

- Function numbers as a slot and a position (two loads per facts lookup): 3 to 7.5 percent on 14 of 20 benchmarks (measured, reverted).
- Splitting the facts into a frozen shared part and a request part: a branch on every call, 2 to 4.5 percent (measured, reverted in #128).
