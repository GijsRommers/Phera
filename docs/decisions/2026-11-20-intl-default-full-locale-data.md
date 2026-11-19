# Keep full compiled ICU4X locale data in the default build

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Owner

## Decision

Keep full compiled ICU4X locale data in the default build.

## Why

Production applications need correct output beyond the 15 test locales. The arm64 release
grew by about 18 MB with intl. The reviewer accepted that cost as smaller than the ICU
library PHP links.

## Turned down

A 15-locale build. ICU4X's baked data spans several components and a comparable subset build
was not measured.
