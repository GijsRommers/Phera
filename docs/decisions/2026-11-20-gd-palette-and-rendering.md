# Store GD colors in their PHP representation

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

GD stores packed truecolor values with its inverted seven-bit alpha,
or eight-bit indices into an image's palette. Conversion to the external
alpha representation happens at the codec boundary. Adaptive color
reduction uses a bounded median-cut palette and optional error diffusion.

## Why

The PHP API exposes palette indices and truecolor values directly through
`imagecolorat()`. Storage should preserve those distinctions and round only
when the file format requires it. Quantization and interpolation algorithms
that can differ from libgd have their own coverage notes.

## Turned down

Using opaque RGBA for every PHP image, which would discard palette indices.
