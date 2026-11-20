# Install in the user's directory

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

The installer puts Phera in `$PHERA_INSTALL/bin`, with `$HOME/.phera` as the
default prefix. It prints a line for the user to add to their shell profile
when the directory is absent from `PATH`.

## Why

A user can install and upgrade Phera with their existing permissions and choose
where to configure their shell.
