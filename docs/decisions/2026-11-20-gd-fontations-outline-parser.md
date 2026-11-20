# Use Fontations to read GD outline fonts

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use `skrifa` from the Google Fonts Fontations project to parse and scale
glyph outlines. Use `ab_glyph_rasterizer` from Alex Butler (alexheretic) to
turn the scaled curves into coverage. The stripped release probe for both
dependencies grew by 387,952 bytes. Adding the rasterizer did not increase
the reported file size at the probe's Mach-O alignment.

## Why

The original `ab_glyph` dependency loads fonts through `ttf-parser`.
RustSec advisory RUSTSEC-2026-0192 marks that parser unmaintained.
The replacement keeps the Rust implementation and uses a maintained
parser for inputs a PHP application may obtain from a user.

## Turned down

Keeping the unmaintained parser under an audit exception.
