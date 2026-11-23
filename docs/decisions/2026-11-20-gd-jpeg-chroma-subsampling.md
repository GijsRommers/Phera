# Match GD JPEG's averaged 4:2:0 chroma sampling

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use `jpeg-encoder` with averaged 4:2:0 chroma sampling, a JFIF density in
dots per inch, and progressive output when interlace is enabled. Volker
Ströbel maintains the crate. The standalone stripped encoder probe grew
by 66,560 bytes.

## Why

PHP's JPEG encoder subsamples chroma. Feeding the same gradient to the
image-rs encoder gave a maximum blue-channel difference of 113 after
PHP decoded both outputs. The averaged 4:2:0 probe gave PHP identical
decoded channels on that gradient.

## Turned down

Using the 4:4:4 encoder exposed directly by the image-rs facade.
