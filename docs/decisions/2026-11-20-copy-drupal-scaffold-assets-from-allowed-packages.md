# Copy Drupal scaffold assets from approved packages

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Read Drupal's file mappings from core, packages named in the root
`allowed-packages` list, and the root manifest. Resolve scaffold locations
within the project. Generate the document-root references to the vendor
autoloaders, including Drupal Runtime for the runtime shim. Create missing
files atomically and preserve existing project files.

## Why

The installed core archive supplies the versioned front controller and other
scaffold files. The [Drupal scaffold hook](https://github.com/drupal/core-composer-scaffold/blob/11.4.13/Handler.php)
copies them and generates two loader references. A create-only first pass
supports fresh scaffolds while protecting project edits during reinstall.

## Turned down

Vendoring scaffold templates or executing the third-party scaffold PHP.
Updating existing files, composing append/prepend mappings and managing
Git ignore rules require additional compatibility and ownership tests.
