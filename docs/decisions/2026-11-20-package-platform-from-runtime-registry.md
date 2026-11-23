# Use the PHP module registry for package platform requirements

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Report the PHP version and PHP modules from the registry that the Phera CLI
uses to run scripts. Use a module's own version when the registry supplies it.

## Why

The package resolver sees the modules the installed binary actually loads,
including modules registered by the runtime itself.

## Turned down

Keeping a second list of supported extensions in the package manager or
consulting the system PHP installation. Both can advertise modules missing
from Phera.
