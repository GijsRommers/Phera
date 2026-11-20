# Pass script event data to PHP callbacks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Pass a Composer Script Event to root PHP callbacks. The initial event supplies
the event name, dev mode and configured vendor directory. A short bootstrap
defines that interface in the callback process.

## Why

Laravel's post-autoload-dump callback accepts a typed event and uses its
Composer configuration accessor to find the project's autoloader. Starting
the callback in a Phera process keeps project scripts on the active runtime.

## Turned down

Loading Composer and its plugin classes in the script process. Plugins have
their own explicit native hooks for the supported package types.
