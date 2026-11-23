# Hold unselected versions during a partial dependency update

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the current lock as the candidate source for each package outside a partial
update. With `--with-all-dependencies`, unlock the named packages' transitive
requirements, including other root requirements. Resolve root manifest edits
together and publish their lock only after resolution succeeds. Keep the new
manifest and lock together if installation fails after publication.

## Why

An update for one package must preserve unrelated package versions and
references. Resolving edits before writing gives the user their original
manifest and lock if the new constraints are unsatisfiable. The published lock
gives install retries a consistent target even if a script or archive fails.

## Turned down

Running a full update for every edit. Restoring the old lock after package files
have already changed would leave the lock and vendor tree describing different
versions.
