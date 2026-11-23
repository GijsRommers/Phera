# SimpleXML views pin nodes in the DOM arena and resolve their selections when accessed

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

SimpleXML views pin nodes in the DOM arena and resolve their selections when accessed. Both extensions report into the same request-local XML error buffer.

## Why

Changes made through either API stay visible to the other, even after importing a node more than once.

## Turned down

Copying trees or maintaining an independent XML model for SimpleXML.
