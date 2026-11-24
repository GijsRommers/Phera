# Enable unread local bindings through the PHPStan feature toggle

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

The bleeding-edge `unusedVariable` setting enables `variable.unused`
and `foreach.unusedKey` at level 4 for local names whose only occurrence
is a plain assignment or a foreach key. Dynamic scope operations and
indirect reads prevent that proof. Unread writes to a name with multiple
uses require a separate data-flow check.
