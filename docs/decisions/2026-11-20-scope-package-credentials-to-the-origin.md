# Scope package credentials to the configured HTTPS origin

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Read Composer authentication from the configured Composer home, the project
`auth.json`, and `COMPOSER_AUTH` in increasing precedence. Match the request
host before adding an authorization header. For Git over HTTPS, set a
host-specific header in the child process and disable HTTP redirects.

## Why

Repository requests can redirect to a separate archive host. Scoping the
credential to the original origin lets the archive download proceed without
sharing it. Git credentials are supplied at invocation time so the mirror's
remote and composer.lock retain ordinary URLs.

## Turned down

Embedding tokens in remote URLs or automatically sending a configured token
to sibling domains. Set a separate auth entry for an API hostname.
