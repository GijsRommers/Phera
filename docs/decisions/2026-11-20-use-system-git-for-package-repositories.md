# Use system Git for package repositories

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Read manifests at tags and branch heads from a cached bare mirror. Lock Git
packages by commit ID and export the commit through the same bounded archive
extractor as distribution packages. Use HTTPS or SSH for remote mirrors.
Local repositories must stay inside the project.

## Why

The system Git client supports configured SSH keys and handles Git transport.
Reading commit objects requires no project checkout during resolution.
A previously fetched mirror can supply the pinned commit offline.

## Turned down

Linking a second Git implementation into Phera. Git archive export honors
repository export attributes and leaves submodule checkout to the project.
