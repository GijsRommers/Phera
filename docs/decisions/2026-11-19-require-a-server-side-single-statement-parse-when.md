# Require a server-side single-statement parse when a mysqli text query contains a statement separator

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Require a server-side single-statement parse when a mysqli text query contains a statement separator.

## Why

The client negotiates multiple statements and exposes no API for toggling that capability after connecting.

## Turned down

Sending multi-statement text through the ordinary query entry point.
