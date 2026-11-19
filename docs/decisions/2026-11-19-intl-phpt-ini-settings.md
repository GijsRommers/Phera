# The PHPT runner accepts date.timezone and intl's request-local INI options

- Date: 2026-11-19
- Area: Tools
- Decided by: Design

## Decision

The PHPT runner accepts date.timezone and intl's request-local INI options.

## Why

The native bindings read these settings, so upstream tests can exercise those paths.

## Turned down

Skipping supported fixtures due to their INI headers.
