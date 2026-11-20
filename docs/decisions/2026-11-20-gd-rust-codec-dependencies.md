# Use Rust codecs for GD uploads and output

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use `image`, `gif`, and `png` from the image-rs project. The `image` features
include BMP, JPEG and WebP only. It uses the Rust `zune-jpeg` decoder, maintained
by Caleb Otieno, and image-rs's `image-webp` for WebP. PNG and GIF are direct
dependencies because they expose original palette indices.

## Why

The codecs have bounded decoding entry points and permit testing against PHP.
The GIF, PNG, BMP, JPEG and WebP format features together added 857,808 bytes
to a standalone stripped release probe. BMP added 83,696 bytes, GIF 102,736
bytes, JPEG 268,848 bytes, PNG 185,680 bytes, and WebP 301,296 bytes when
measured one at a time. The combined probe shares code among the codecs.

## Turned down

C decoder dependencies and the complete set of default image-rs format
features.
