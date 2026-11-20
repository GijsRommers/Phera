# `XMLWriter` writes from a stack of open XML constructs and shares XML name validation with DOM

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

`XMLWriter` writes from a stack of open XML constructs and shares XML name validation with DOM. It uses Tokio for file output and the native stream layer for PHP stream output.

## Why

PHP can flush an unfinished start tag, write raw bytes, and emit fragments without building a document.

## Turned down

Building a DOM arena and serializing it on flush.
