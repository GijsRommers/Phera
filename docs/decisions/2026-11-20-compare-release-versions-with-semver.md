# Compare release versions with SemVer

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use GitHub's latest stable release by default and compare it with the
installed version using Semantic Versioning precedence. An explicit version
can select a prerelease or an older release. Accept the version with or
without the tag's leading `v`.

## Why

Cargo versions use SemVer. Numeric comparisons and prerelease ordering matter
when deciding whether to replace the installed executable.
