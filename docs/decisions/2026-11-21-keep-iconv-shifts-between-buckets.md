# Keep iconv shifts between buckets

- Date: 2026-11-21
- Area: Stream filters
- Decided by: Design

## Decision

The iconv extension can give a stream filter its own converter through the
extension registry. Conversions between UTF-8 and ISO-2022-JP use the
existing Rust encoding library to carry shift state between incoming
buckets. A flush resets that state. Staged output is bounded per bucket.
The existing byte converter handles the other charset pairs and the
standalone iconv functions.

## Why

ISO-2022-JP escape sequences apply to the characters that follow, including
those in the next bucket. Treating each bucket as a complete input loses
that state during decoding and inserts extra resets during encoding.
Keeping the converter on the filter uses bounded memory and preserves the
stream order.
