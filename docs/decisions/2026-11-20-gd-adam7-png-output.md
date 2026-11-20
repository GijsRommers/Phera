# Write GD interlaced PNGs as Adam7 passes

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

When `imageinterlace()` is enabled, GD writes Adam7 rows and streams them
through the existing `flate2` zlib encoder into PNG IDAT chunks. The
`png` crate still writes the header, palette, transparency, and checksums.

## Why

The PNG crate decodes Adam7 and can declare it in the image header. Its
high-level encoder takes ordinary rows without building the seven passes.
GD can assemble each pass a row at a time, using the same bounded pixels
with no second full raster and supporting PNG compression and row filters.

## Turned down

Writing a non-interlaced PNG when the image requests Adam7, or adding
another PNG library for the output path.
