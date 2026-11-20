# Keep the installer at the repository root

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

The public installer is the single POSIX shell file `install.sh` at the
repository root. Its fake release test lives in the CI workflow.

## Why

The installer needs a stable URL that works before Phera is installed. Keeping
it at the root is an exception to the rule for loose helpers.
