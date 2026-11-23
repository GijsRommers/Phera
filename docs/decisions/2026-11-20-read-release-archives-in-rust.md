# Read release archives in Rust

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

The upgrader uses the existing flate2 dependency with tar-rs to read the
regular `phera` member of the release archive. It writes that member to a
file it created in the install directory.

## Why

Phera can update itself on machines without a tar command. Using a tar reader
avoids maintaining a parser and gives archive entries no control over the
destination path.
