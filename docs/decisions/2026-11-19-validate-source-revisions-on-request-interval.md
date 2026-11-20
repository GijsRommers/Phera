# Validate source revisions on the request interval

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Codex

## Decision

Start timestamp intervals from the request start. Expose PHP's three cache validation directives in the server SAPI and coordinate compilation by canonical file and source revision.

## Why

The worker needs no clock access per cached include. Different files can compile in parallel, and the next check sees replaced or removed files.

## Turned down

Registering a Zend OPcache extension, caching HTTP route selection across requests, or locking during a warm lookup.
