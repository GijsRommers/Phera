# Use the runtime TLS policy for package downloads

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use the I/O pool's ring-backed rustls configuration for the package client's
TLS connections. Honor the project-level Composer `secure-http` setting.
Attach credentials to HTTPS requests at the original origin only.

## Why

Package downloads share Phera's TLS stack. Keeping credentials on the original
origin covers redirects to both a different host and a different port.

## Turned down

Building a second TLS configuration and resending authorization based on a
hostname match alone.
