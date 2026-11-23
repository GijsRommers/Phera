# Share Unicode codecs with UConverter

- Date: 2026-11-20
- Area: intl encoding conversion
- Decided by: Design

## Decision

Reuse mbstring's Unicode byte codecs for UTF-8, UTF-16 and UTF-32. Use ICU
single-byte maps and alias metadata for the ASCII, ISO-8859 and Windows
1250 through 1258 converters PHP can open. Apply ICU's byte-order and
substitution behavior in the UConverter binding.

## Why

Symfony's emoji transliterator converts UTF-8 to UTF-16BE to identify
surrogate pairs at slice boundaries. ICU differs from web encodings for
several ISO-8859 labels, and ambiguous Windows labels produce observable
status messages. The tables add 28,593 bytes of source data.

## Turned down

Routing every label to the host iconv or using WHATWG label semantics for
ICU code pages.
