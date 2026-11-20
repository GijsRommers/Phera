# Preserve absolute candidates when the working directory is gone

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Keep absolute include candidates when the working directory is unavailable.

## Why

PHP can still find a file through an absolute include path or the including file's directory after the working directory has been deleted.

## Turned down

Abort the search for a relative spelling when the working directory is missing.
