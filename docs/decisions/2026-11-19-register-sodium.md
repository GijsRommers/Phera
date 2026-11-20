# Register Sodium as a native extension

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Register Sodium and SodiumException through the extension API. Load Sodium in the hosted PHP
oracle.

## Why

Applications need the PHP function surface, reference arguments and exception messages.

## Turned down

A separate extension loader or an application-only compatibility layer.
