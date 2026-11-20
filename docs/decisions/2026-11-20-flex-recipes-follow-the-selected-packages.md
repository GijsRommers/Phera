# Apply Symfony recipes to the packages selected by the project

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Plan native Flex support as recipe data applied after dependency selection.
Select the newest compatible recipe from the official index with the same
package-version rule as Flex. Honor `symfony.lock` pins when replaying a recipe.
For framework bootstrap, start with recipe file copies, bundle registration,
configuration placeholders and dotenv configuration. Preserve project edits.

## Why

The [Flex downloader](https://github.com/symfony/flex/blob/v2.12.0/src/Downloader.php)
resolves recipes against the selected package version. The framework-bundle
recipe supplies the public entry point and Kernel. The official skeletons
at `v8.1.99` and `v6.1.99` declare Flex alone, so an unchanged lock containing
only Flex has no framework-bundle recipe to apply. The boot fixtures need a
manifest and lock for the framework and webapp dependencies they exercise.

## Turned down

Synthesizing a framework application or silently replacing dependencies in a
frozen Flex-only lock. Applying arbitrary recipe PHP or running package code
while resolving also remains outside the native hook.
