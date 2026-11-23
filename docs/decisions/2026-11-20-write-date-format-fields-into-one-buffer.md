# Write date format fields into one buffer

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Append date format fields directly to the output bytes. Write the common
two-digit fields as byte pairs and reuse Rust's formatting for variable
widths and composite fields.

## Why

A format can consist of many short pieces. Giving every piece its own
string allocates even for fixed punctuation and numbers. The output still
accepts byte format strings, including NUL and invalid UTF-8.

## Turned down

Building a temporary string for each field or introducing a separate
format implementation for date objects.
