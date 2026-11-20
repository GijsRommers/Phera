# Copy GD images and fileinfo flags when a request forks

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Register the plain-data fork policy for both `GdImage` and `finfo`.
A fork copies the image's raster, palette, and rendering options. It
copies the fileinfo handle's flag value. Both copies belong to the request.

## Why

These objects hold neither a PHP object reference nor an open resource.
Drawing on an image and changing a handle's flags are ordinary request
operations, and their changes stay local even when the boot made the object.

## Turned down

Sharing mutable pixels or flags between requests. PHP-level clone and
serialization keep their existing behavior.
