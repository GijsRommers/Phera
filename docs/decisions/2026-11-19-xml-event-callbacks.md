# `XMLParser` delivers callbacks as complete tokens arrive and shares DOM's bounded entity resolver

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

`XMLParser` delivers callbacks as complete tokens arrive and shares DOM's bounded entity resolver. The PHP entry points follow the libxml SAX conventions.

## Why

WordPress uses chunked parsing and calls into PHP during parsing. The resolver keeps external identifiers inert.

## Turned down

Building a complete tree before calling handlers or linking Expat.
