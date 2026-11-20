# Stage framework starters with the native installer

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Fetch the official starter package through the package cache. Install
dependencies in a staging directory and publish the project when that
succeeds. Run setup scripts from the published path. Try Composer when
native template preparation or dependency installation refuses the project.

## Why

The starter follows official release packages, and Composer can use the
requested directory if Phera cannot create the project. Symfony and Laravel
scripts can generate files tied to an absolute working directory, so those
scripts run at the final location.

## Turned down

Keeping framework source copies in the binary or requiring Composer for every
new project.
