# Destructors run before control leaves PHP code

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Due destructors also run at three places outside the interpreter's instruction boundaries:
- when a callback that native code called returns normally (`array_map`, `usort`, `call_user_func`, output and shutdown callbacks, autoloaders, exception handlers), before the native code goes on;
- right after released generators are closed, so their variables are destructed before the next statement;
- when a released generator wakes the destructor check, as a queued destructor does.

At the end of the request, generators still alive are closed in the destructor phases (globals, then by id), which runs their `finally` blocks.

## Why

PHP destructs an object when its last reference goes. A callback's variables go when it returns, which is before control is back in the interpreter. The review of #53 found locals of shutdown functions and exception handlers destructed one callback late, and a generator's locals one statement late. php-src `generators/exception_during_shutdown` expects the force-close at the end of the request.

## Turned down

Draining at the next instruction boundary only, the earlier behaviour. It is too late for a callback whose native caller does more PHP work before returning, such as the next shutdown function.

Giving every generator a due destructor, as fibers have. That would match PHP's order of object ids too, but every script with a live generator would run the guarded loop.
