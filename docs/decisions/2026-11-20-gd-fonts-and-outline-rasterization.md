# Use libgd's bitmap fonts and Rust outline glyphs

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

The five built-in fonts use the original libgd bitmap data and its license.
The outline methods use `ab_glyph`, maintained by Alex Butler (alexheretic),
for font parsing, layout metrics and glyph coverage. The stripped release
probe for font decoding and glyph rendering grew by 101,984 bytes.
Outline input is bounded to 16 MiB of font data and the glyph footprint is
checked before allocating coverage.

## Why

The static bitmaps can match PHP pixel for pixel. The outline rasterizer
supports the common font formats without linking FreeType, while FreeType's
hinting and metrics differ.

## Turned down

Bundling a C font parser for the outline API.
