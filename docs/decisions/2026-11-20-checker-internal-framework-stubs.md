# Account for framework stubs in internal class checks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Treat Doctrine's lazy repository base as public when the configured
Doctrine extension supplies its replacement stub. For a configured
Symfony application, leave internal-class ignores on calls to
`IntlDateFormatter` unverified.

## Why

The Doctrine stub removes the internal tag from the repository base.
PHPStan can also resolve the Symfony ICU polyfill's internal class
behind a native-looking `IntlDateFormatter` call. Phera resolves that
call against its native declarations.

## Turned down

Reporting vendor declarations as internal after a configured stub
replaces that metadata, or claiming the polyfill ignore is unused.
