# Run declared Symfony auto-scripts through Phera

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When the project allows Symfony Flex, dispatch its declared auto-script types
from composer.json. Use the current Phera binary for PHP and Symfony console
commands. Shell scripts run in the project directory. Propagate a failed
script's status and keep `--no-scripts` available for installation retries.

## Why

Symfony skeletons use Flex to expand configurable directories and call the
framework's console after autoload generation. The
[Flex executor](https://github.com/symfony/flex/blob/v2.12.0/src/ScriptExecutor.php)
defines the behavior of these three declared command types.

## Turned down

Loading the PHP Flex plugin to dispatch commands. Substituting a different PHP
interpreter would change the runtime the project script is intended to exercise.
