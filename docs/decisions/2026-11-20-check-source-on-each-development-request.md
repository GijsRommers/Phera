# Check source on each development request

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

With `phera serve --dev`, enable timestamp validation and set the interval to zero after reading startup INI.

## Why

Developers see source edits on the next request. The development setting applies to the front controller and includes.

## Turned down

Keeping the production validation interval or startup overrides that leave source frozen in development mode.
