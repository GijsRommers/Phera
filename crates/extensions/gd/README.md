# GD

Phera ships a Rust implementation of GD for PNG, JPEG, GIF, BMP and WebP.
`GdImage` stores the pixels and rendering options of each image. The
`getimagesize()` family and `exif_imagetype()` inspect headers independently
of full decoding.

- PNG and GIF preserve palette indices when reading. JPEG and WebP lossy
  decoding can differ by rounding. The PHP oracle checks all decoded channels.
- WebP output uses the lossless encoder for every accepted quality. GD's
  quality-dependent WebP encoding, animated image output and AVIF decoding
  and encoding are outside this build.
- Truecolor drawing, geometric transforms and integer filters have checksum
  comparisons. Adaptive palette quantization, arbitrary rotations, antialiasing,
  higher-order interpolation and font rasterization can differ from libgd.
- The built-in font bitmaps come from libgd. Outline text uses a Rust
  rasterizer and has different font hinting, glyph positions, and coverage
  from FreeType. The outline loader reads explicit font paths.
- PNG output supports Adam7 interlacing. Metadata probes cover the main web
  image types and a subset of the dimension parsers in PHP's catalog.
- Images are limited to 16,777,216 pixels and 32,768 pixels on either axis.
  Named decoders read at most 64 MiB of encoded input.

The [libgd license and bitmap provenance](data/README.md) cover the GD
material used by this crate.
