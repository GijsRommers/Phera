# Modern DOM classes share the existing arena and use html5ever for HTML5 and Servo's Rust selector matcher

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Modern DOM classes share the existing arena and use html5ever for HTML5 and Servo's Rust selector matcher.

## Why

Modern and legacy PHP class hierarchies have separate wrappers. The same tree can still serve both APIs, and selector matching needs no tree copy.

## Turned down

A second tree for modern DOM or a C selector engine.
