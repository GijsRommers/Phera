# `XMLReader` keeps a forward cursor over the shared DOM arena

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

`XMLReader` keeps a forward cursor over the shared DOM arena. The parser records the spelling of
empty tags and DTD default attributes. A reader retains a caller-provided stream until close,
while an expanded DOM node owns its copied subtree and inherited namespace declarations.

## Why

Reader properties and expansion reuse a single XML and namespace model.

## Turned down

A separate XML parser behind the reader.
