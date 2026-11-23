# Store SQL result cells contiguously

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Keep PHP cells in one flat allocation indexed by row and column. PDO
shares column keys across rows and assembles the requested PHP shape
when a row is fetched. Fetches move cells that cannot be read again.
MySQL buffered PDO converts one client row at a time and releases each
client row before reading the next. Reserve space when the client has
already supplied a row count.

## Why

The previous representation had a separate vector of PHP values per row.
PDO also rebuilt column keys for every fetch and cloned cells before
forming PHP arrays or objects. Fetch mode remains a per-call decision,
and mysqli supports seeking back to an earlier buffered row.

## Turned down

Building PHP arrays or objects during execute before PHP specifies how to
fetch a row, or replacing either protocol client in this change.
