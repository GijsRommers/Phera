# Build release archives natively

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Build on GitHub's native macOS and Ubuntu arm64 and x86_64 runners.
Use Ubuntu 22.04 for a glibc 2.35 build baseline. Package the binary and both
licences in a gzip-compressed tar archive. A manual dry run uses the selected
branch and uploads workflow artifacts.

## Why

Native builds use the runner's compiler for bundled PCRE2 and SQLite and can
run the resulting executable before packaging. Dry runs exercise those builds
before the owner creates a tag.
