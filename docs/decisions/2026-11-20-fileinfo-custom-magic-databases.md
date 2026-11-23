# Keep fileinfo's classifier independent of external magic databases

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

The default fileinfo classifier is available through both the procedural and
object APIs. PHP's custom magic databases remain unsupported.

## Why

Loading arbitrary rule sets adds a parser and an evaluator for a second input
format. Symfony and Laravel use the default database for their default MIME
guessers. The shared upload path can stay bounded and avoid that parser.

## Turned down

Interpreting libmagic database entries supplied by the application.
