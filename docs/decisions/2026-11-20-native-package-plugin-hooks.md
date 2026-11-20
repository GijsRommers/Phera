# Apply Composer plugin effects through native hooks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Give each supported Composer plugin a Rust hook for its data-driven install
operation. Use the root `config.allow-plugins` boolean or first matching
package rule before activating the hook. An unlisted required native hook
reports how to allow or explicitly skip it. `--no-plugins` skips the native
hooks. Hooks run only after selecting the dependencies.

## Why

Frameworks depend on generated files and package locations that Composer
plugins provide. Phera can produce these effects from manifests, templates
and archives. Composer's [plugin manager](https://github.com/composer/composer/blob/2.11.2/src/Composer/Plugin/PluginManager.php)
uses ordered allow rules and blocks unlisted required plugins in noninteractive
installs.

## Turned down

Loading third-party plugin PHP into the resolver or emulating the Composer
objects that plugins receive. Interactive trust prompts, global plugin
configuration and the pre-2.2 lock exception are separate compatibility work.
