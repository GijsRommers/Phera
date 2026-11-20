# Encode GD WebP output losslessly

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

GD uses image-rs's lossless WebP encoder. The PHP quality argument is
accepted and checked at the call boundary. Quality does not select lossy output.

## Why

The established image-rs backend can decode both WebP modes and can
encode lossless files. It supports the lossless constant and preserves the
full GD color channels. Files that PHP would encode lossily can differ in
size and decoded pixels.

## Turned down

Linking libwebp and adding the current AGPL or commercially licensed
zenwebp encoder.
