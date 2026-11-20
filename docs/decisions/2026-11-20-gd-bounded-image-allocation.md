# Check GD dimensions before allocating pixels

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

GD accepts at most 16,777,216 pixels and 32,768 pixels per axis. The rectangle
fits in 64 MiB of packed truecolor pixels or 16 MiB of palette indices.
Decoders limit named encoded input to 64 MiB and check dimensions before
allocating the full raster. Format adapters also limit decoder-owned buffers
where the codec exposes limits. Image I/O and codec work leave the request
thread while operating on owned Rust bytes.

## Why

A tiny compressed upload can claim a large raster. The bounds apply to
creation and transforms as well as to decoders. Separate axis limits keep
row and column scratch space bounded, even for an image with one short axis.
Exceeding a decoder limit returns false and warns through the image function.

## Turned down

Trusting a decoder's default dimensions and reserving from an upload header
before checking total pixels.
