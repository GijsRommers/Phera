# Recognize streamed Office ZIP headers without unpacking

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

For Office ZIPs written to a stream, use at most 256 KiB per ZIP entry to
find the next local or central header. A size in the data descriptor also
allows inspecting the uncompressed OpenDocument `mimetype` entry.

## Why

A non-seekable ZIP writer can declare the entry size after the data. The
ordinary local header's length is then unavailable. The Office markers
needed for identification live in early filenames or in a stored MIME entry.

## Turned down

Inflating uploaded archives to determine their file type.
