# Phera has no template engine of its own for now; Twig and Blade run as PHP

- Date: 2026-11-18
- Area: Tools
- Decided by: Design

## Decision

Phera has no template engine of its own for now; Twig and Blade run as PHP. A native renderer is designed (architecture.md 11.6) and not built.

## Why

The frameworks' own engines must work anyway.
