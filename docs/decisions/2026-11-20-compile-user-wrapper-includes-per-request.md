# Compile user wrapper includes per request

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

The include loader asks the request's wrapper for PHP source and uses the
wrapper's opened path for include-once bookkeeping. Compiled wrapper source
does not enter the filesystem include cache. PHP's URL protections apply
to registered URL wrappers before their callbacks run.

## Why

Two requests can register different classes for the same scheme, and the
same wrapper can return different source for the same path.

## Turned down

Using the path of an application wrapper resource as a worker-wide
compiled-code cache key.
