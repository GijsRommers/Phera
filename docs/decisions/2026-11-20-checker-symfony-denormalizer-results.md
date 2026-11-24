# Follow the Symfony extension for denormalizer interface results

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When a project includes the Symfony PHPStan extension, keep the
result of a method declared by `DenormalizerInterface` unresolved.
Keep the type of an overriding method when the declaration supplies it.

## Why

The extension's stub replaces Symfony's generic return annotation with
mixed. An object check on the configured result can therefore be useful.

## Turned down

Reporting that every denormalizer call must return an object or
discarding return types on unrelated serializers.
