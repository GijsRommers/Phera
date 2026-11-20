# Plan framework package paths before installing

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use a single package location for extraction, bin proxies, the PHP autoloader
and installed metadata. Apply the supported `composer/installers` Drupal and
WordPress paths and the `roots/wordpress-core-installer` core path behind their
respective `allow-plugins` entries. Reject path overlaps and existing custom
directories without a matching installed record.

## Why

[Composer Installers](https://github.com/composer/installers/blob/v2.3.0/src/Composer/Installers/BaseInstaller.php)
uses the first package, type or vendor match in the root install paths.
[Roots](https://github.com/roots/wordpress-core-installer/blob/v4.0.0/src/WordPressCoreInstaller.php)
checks root overrides before package defaults. Resolving the locations once
keeps runtime lookups consistent and catches unsafe layouts before copying.

## Turned down

Installing a second copy in the framework path while leaving the actual
package record in vendor. Uninstall and relocation of stale custom paths
needs separate ownership tracking. Existing custom directories are preserved
when they fall out of the plan.
