# Compare generated Composer file bytes separately from their behavior

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Report differences for the root loaders, installed JSON and every direct PHP
file in `vendor/composer`. Record which file is absent, or the first differing
byte offset and both lengths. The optional strict-byte gate fails on any of
these differences in addition to the existing package tree and lookup gates.
Match Composer's presence rules for empty eager-file maps and disabled platform
guards. Reuse its loader suffix from config, an existing loader or a valid lock
hash, in that order.

## Why

The semantic checks help identify runtime regressions, while a byte report
exposes formatting, path expressions, metadata and other generated differences.
Composer's [AutoloadGenerator](https://github.com/composer/composer/blob/2.11.2/src/Composer/Autoload/AutoloadGenerator.php)
defines the file presence and suffix rules.

## Turned down

Treating equivalent lookups as evidence of identical files. Keep the
checks independent so neither can mask differences in the other.
