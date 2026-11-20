# Use HTTPS for upgrade downloads

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

The upgrader uses Hyper over Phera's ring-backed rustls configuration. It
accepts HTTPS release mirrors and follows redirects only to GitHub's HTTPS
release hosts. Mirrors serve the latest metadata and assets at the same
relative paths as the test fixture.

## Why

Phera can reuse its existing HTTP and certificate handling. The explicit host
list limits where redirects can send an upgrade request.
