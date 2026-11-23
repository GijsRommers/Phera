# Read PCRE settings from request state

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Write PCRE's three typed settings using the existing INI update hooks at
startup, on a runtime write, and on restoration. The extension reads the
typed settings from its request state and retains the existing compiled
pattern key. A request without changes uses the default fields.

## Why

The match path no longer looks up three INI names or reconstructs and parses
their text. The request state follows the settings into a fork, and the INI
registry still provides the text when PHP reads it.

## Turned down

Keeping prebuilt PHP strings for INI defaults. Sharing static or short
strings belongs with the string representation.
