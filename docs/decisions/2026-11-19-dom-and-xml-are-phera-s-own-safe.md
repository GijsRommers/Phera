# DOM and XML are Phera's own safe tree with pure Rust parsers (`quick-xml`, `html5ever`), not libxml2

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

DOM and XML are Phera's own safe tree with pure Rust parsers (`quick-xml`, `html5ever`), not libxml2. No external entity is ever loaded while parsing a string. `DOMDocument::loadHTML()` therefore differs from PHP for malformed HTML 4.

## Why

A C parser fed with documents from the network gives up the memory safety the runtime exists for.

## Turned down

Binding libxml2 for byte-exact output.
