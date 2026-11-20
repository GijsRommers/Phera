# Preserve stable XMLParser positions across platforms

- Date: 2026-11-20
- Area: XML
- Decided by: Design

## Decision

The pure Rust XMLParser reports columns using the libxml 2.9.13 convention
advertised by Phera. Libxml 2.15.2 omits the opening entity-value quote from the
column count. The oracle checks both exact raw columns for that case according
to the loaded library version, then compares the full callback trace, status,
line, and byte position.

## Why

The same PHP 8.5 XML extension reports different columns when linked to these
two libxml versions. Phera has one parser on all platforms. An application
should see the same source locations on each of them.

## Turned down

Inferring a libxml version from the operating system or ignoring the reported
column in oracle comparisons.
