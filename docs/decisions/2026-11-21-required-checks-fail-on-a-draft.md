# The required test checks fail on a draft instead of being skipped

- Date: 2026-11-21
- Area: Process
- Decided by: Design

## Decision

The two long required checks, `Test (Linux, PHP 8.5, PostgreSQL 17, MySQL 8.4)`
and `PDO end to end`, are small jobs that pass only when the jobs behind them
ran and passed. On a draft pull request the long jobs still do not run, and
the two checks fail with the message "Draft pull request: the long test jobs
do not run. Mark it ready to run them." A draft therefore shows two failed
checks by design; the short checks (format, clippy, the guard against
`unsafe`, advisories) say whether work in progress is in order.

The reviewer sets a pull request to merge only after seeing the long jobs
running or passed on its current head.

## Why

A skipped job counts as passed for a required check. On 2026-11-21 a branch
was updated with main and marked ready in the same minute. Two runs started
for that head: one for the push, while the pull request was still a draft,
and one for becoming ready. They share a concurrency group, the full run was
cancelled, and the draft's run finished with the long jobs skipped. Auto-merge
merged the pull request ninety seconds later, and main failed `PDO end to end`
on a stream script that differs from PHP on Linux. The merge was reverted.

## Turned down

- Keeping the skip and relying on the order of the two runs: which run is
  cancelled is not ours to decide, and a window in which the skipped result
  counts remains.
- Running the long jobs on drafts again: work in progress is pushed often and
  kept finished pull requests waiting for a runner, which is why drafts
  got the short checks only.
