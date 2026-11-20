# Load Composer package include paths through the autoloader

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Collect each installed package's `include-path` entries into a relocatable
`vendor/composer/include_paths.php` file. Load it before registering the
autoloader. Resolve each entry inside its package directory.

## Why

Current projects still install dependencies that declare include paths.
Drupal's recommended project brings in PEAR libraries that rely on them.

## Turned down

Ignoring the declared paths during autoload generation. The resulting loader
would give PHP a different include path from the same manifest under Composer.
