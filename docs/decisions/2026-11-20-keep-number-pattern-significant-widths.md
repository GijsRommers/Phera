# Preserve significant digits in number patterns

- Date: 2026-11-20
- Area: intl number formatting
- Decided by: Design

## Decision

Use the significant-digit widths from the pattern when formatting and
reporting its attributes. Keep ICU's 100-character readback limit for
all-`@` patterns separate from the actual formatting precision.

## Why

Applications can request more than 100 significant digits and observe
that precision in the output. PHP's pattern getter uses ICU's shorter
serialized representation.

## Turned down

Applying the pattern getter's length limit to formatting.
