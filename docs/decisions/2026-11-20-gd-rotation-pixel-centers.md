# Use GD 2.3.3's fixed-point pixel centers for rotation

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Use integer raster centers and eight fractional bits for arbitrary rotations,
following the external libgd 2.3.3 used by the Homebrew PHP oracle. Interpolated
rotation samples around these coordinates using the selected kernel.

## Why

A different image center or rounding rule moves a visible part of the
rotation by a pixel. libgd's nearest-neighbour and interpolation paths
use different offsets and different treatment of the border.

## Turned down

Applying conventional geometric centers to every rotation. PHP built with
its bundled GD uses a different arbitrary rotation offset and bounding
rectangle. The external libgd behavior remains the compatibility target
for this path.
