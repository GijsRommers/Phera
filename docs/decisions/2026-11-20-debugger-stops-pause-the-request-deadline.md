# Debugger stops pause the request deadline

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

On a debug-enabled server, when a request waits for commands from an
attached IDE, its wall-clock budget pauses. The wall-clock timer continues
during application work and the initial attempt to connect to the IDE.
PHP's CPU-time accounting already excludes time spent parked on a socket.

## Why

An engineer can inspect a stopped request for as long as needed. Once it
resumes, a slow database query or a runaway script still has a deadline.
Tokio can run other requests on the same worker throughout the stop.

## Turned down

Disabling the wall-clock request limit for all requests on a debug-enabled
server.
