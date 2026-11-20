# Debug connections are outbound and opt-in

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

A debug session connects to the configured IDE endpoint, which defaults to
`127.0.0.1:9003`. Request triggers may start sessions on an enabled server.
Request headers never choose the endpoint. The server warns when step
debugging is enabled. A non-loopback listen address also requires
`--allow-remote-debug`, including when configuration enables debugging.

## Why

An IDE can inspect sensitive variables or execute PHP in an attached
request. The server operator must choose who can reach such a deployment
and where its debugger connections go.

## Turned down

An inbound debugger listener and header-based client discovery.
