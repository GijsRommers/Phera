# Skip temporary work when making reflected callables

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Borrow the callable name from its existing metadata. Only retain a closure
when making a new reflector for a closure or its invoke method.

## Why

A newly allocated reflected callable has no old retained receiver to
clear. Removing that property allocated a key for every method. On the
route probe, a `getMethods` call went from 115,437 to 104,273 retired
instructions. Rebinding an existing reflection object can still need to
clear the previous receiver.

## Turned down

The member lookup and method linking work belongs in the engine. This
change keeps both boundaries intact.
