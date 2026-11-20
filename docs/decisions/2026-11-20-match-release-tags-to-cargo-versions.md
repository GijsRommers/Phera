# Match release tags to Cargo versions

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Cargo's workspace version is authoritative. A published release requires an
existing tag named `v<version>`, and each built executable must report that
version. A dry run uses the tag input as its expected version.

## Why

The filename, executable and GitHub release should describe the same build.
A version check before the matrix catches a mismatch without compiling four
binaries.
