# Classic zlib byte compatibility

- Date: 2026-11-21
- Area: Stream filters
- Decided by: Design

## Decision

The deflate filter uses the existing flate2 encoder until a supported safe
streaming interface to classic zlib provides the window and memory settings
needed for PHP byte compatibility.

## Why

The 100 MiB PHP oracle emits 305,047 bytes with memory level 9. The
existing encoder emits 305,144 bytes. The zlib-rs trial emits 693,581 bytes
with the same input and settings and also chooses different back-references
for the small oracle string. All three streams decompress to the same data.
Replacing the encoder needs an interface that can reproduce PHP's bytes at
both sizes.
