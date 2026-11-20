# Use the installed Symfony Runtime bootstrap template

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Substitute runtime class, project directory and options into the template
shipped by the installed `symfony/runtime` package. Write the result during
the post-autoload phase. A project may select a template inside its own tree.

## Why

The bootstrap belongs to the installed Runtime version. The [Symfony hook](https://github.com/symfony/runtime/blob/v7.4.23/Internal/ComposerPlugin.php)
generates the same file from package and project data after the class loader
exists. Reading the installed template keeps the Rust hook small.

## Turned down

Maintaining a separate copy of Symfony's bootstrap in Phera or requiring a
Composer invocation to generate it.
