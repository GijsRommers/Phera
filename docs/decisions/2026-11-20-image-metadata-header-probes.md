# Inspect image dimensions independently of GD decoding

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

The built-in image metadata helpers read bounded headers. The runtime
registers the metadata functions in PHP's standard module and
`exif_imagetype()` in exif. The exif module provides the type probe only.

## Why

Inspecting uploads for dimensions or a type needs neither a decoded image
nor an EXIF tag parser. It can also recognize types GD cannot decode.

## Turned down

Running a full image decoder for each size query.
