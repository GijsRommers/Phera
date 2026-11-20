# Filter repositories before canonical selection

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Apply a repository's `only` and `exclude` package patterns before looking up
versions. Once a canonical repository supplies a package name, finish that
lookup using the candidates already collected. Apply the same rules when
finding packages that provide or replace a virtual dependency.

## Why

Projects use repository order to prefer a private fork or route a package
namespace to its own registry. The selection has to apply consistently to
direct package names and virtual requirements.

## Turned down

Filtering downloaded candidates after picking a repository. An excluded
name needs to remain available from a lower-priority repository.
