# When a web request ends, terminate an abandoned child and its process group while Phera still tracks the group leader

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

When a web request ends, terminate an abandoned child and its process group while Phera still tracks the group leader.

## Why

A child can retain descriptors and consume resources after the response or deadline of a request.

## Turned down

PHP-FPM leaves ordinary children running. Waiting for them would tie request cleanup to the life of the child.
