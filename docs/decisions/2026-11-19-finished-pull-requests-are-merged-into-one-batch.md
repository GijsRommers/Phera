# Finished pull requests are merged into one batch branch, reviewed together and merged with one CI run

- Date: 2026-11-19
- Area: Process
- Decided by: Design

## Decision

Finished pull requests are merged into one batch branch, reviewed together and merged with one CI run. A batch merges only when a run of all php-src tests shows no test going from pass to fail.

## Why

Every merge makes the other branches out of date; the full php-src run finds regressions the repository's own selection does not.

## Turned down

Merging one by one.
