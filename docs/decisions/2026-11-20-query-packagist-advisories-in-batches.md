# Query Packagist advisories in bounded batches

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use Packagist's advertised `https://packagist.org/api/security-advisories/`
endpoint. POST URL-encoded indexed `packages` fields in batches of at most
500 names. Match each result's `affectedVersions` against the locked version
with Phera's Composer constraint matcher. Report active and individually
ignored advisories independently. An unreachable endpoint is an audit error.

## Why

[ComposerRepository](https://github.com/composer/composer/blob/2.11.2/src/Composer/Repository/ComposerRepository.php)
requests full advisories in these batches and matches each affected range to
the package version. [Auditor](https://github.com/composer/composer/blob/2.11.2/src/Composer/Advisory/Auditor.php)
recognizes Packagist advisory IDs, CVEs and source IDs when filtering ignores.
The [Packagist repository index](https://repo.packagist.org/packages.json)
advertises the API URL.

## Turned down

Treating an unavailable feed as a clean result or scraping human-facing CVE
pages. Install/update audit summaries and repository-specific advisory feeds
can share the request and matching code when added.
