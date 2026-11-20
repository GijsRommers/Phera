# Preload Composer’s generated maps before listening

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Read Composer's class map and files lists as PHP AST data and compile their canonical files before listening with up to four large-stack blocking workers.

## Why

The lists are already generated at install time. A worker can reuse its compiler stack for multiple files, and application PHP still executes at request time.

## Turned down

Executing Composer metadata, recursively scanning PSR-4 trees, or emulating Zend's executable `opcache.preload` script.
