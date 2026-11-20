# Keep the previous executable

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

The upgrader stages the verified binary in the install directory, replaces the
installed file with a rename, and saves the old binary there as `phera.old`.
Upgrades lock the directory while preparing and installing the replacement.
Known package-manager paths and unwritable installs get an explanation before
the archive is downloaded.

## Why

A same-filesystem rename exposes the complete binary in one step. Keeping the
previous executable gives the user a local recovery option.
