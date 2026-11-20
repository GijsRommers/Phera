# Fork XML data within a request and refuse open XML I/O

- Date: 2026-11-20
- Area: Runtime
- Decided by: Design

## Decision

Each fork copies a SimpleXML document once, so its views and active cursors share the copy.
An XMLReader can copy buffered input or a parsed cursor. A memory XMLWriter copies its buffer
and stack. An idle XMLParser copies its parser state and forks its PHP callbacks.

An XMLReader holding an open stream and an XMLWriter targeting a URI or stream refuse a
snapshot. An XMLParser currently dispatching a callback also refuses. Writer finalization
queues are empty in a fork.

## Why

Forked requests need independent tree, parser, and output mutations. Sharing a process handle
would also share its stream position or output, and reopening it could truncate a file.

## Turned down

Sharing mutable XML state between requests, copying one arena per SimpleXML view, or reopening
the boot's XML streams.
