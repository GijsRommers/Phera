# Match intl input validation at the PHP entry point

- Date: 2026-11-20
- Area: PHP compatibility
- Decided by: Design

## Decision

Apply the 156-byte locale limit at the same entry points as PHP. Constructors
that use the common ICU locale error throw an IntlException and record the error.
Date and calendar time zone arguments throw an IntlException with the supplied
identifier when lookup fails. IntlTimeZone creation keeps its unknown-zone
fallback.

## Why

PHP uses distinct validation paths for locale inputs and failed time zone lookup.
Applications can observe both the exception and the request's intl error state.

## Turned down

A locale limit applied to all ICU callers and a shared generic result for an
unrecognized time zone.
