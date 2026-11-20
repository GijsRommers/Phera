# Composer differential results

## Native locked-install checkpoint

Composer 2.11.2 and PHP 8.5.12 on macOS. All 25 original locked fixtures match
package files, parsed installed.json and installed.php, and PHP autoload lookups.
The generated PHP and installed JSON files match byte for byte across all 25
fixtures. Phera's runtime probe matches 23. Guzzle requires stream filters
tracked in #122. Slim emits a different nullable-parameter deprecation.

The framework run enabled approved plugins and disabled root scripts for the
file comparison. The boot run installed with root scripts enabled and Composer
absent from PATH. It used the official Symfony 8.1 skeleton and webapp
dependency locks, with the native installer laying out the pinned recipe
files. The copied applications stayed in isolated directories.

| Framework | First strict platform rejection | Phera HTTP | PHP 8.5 HTTP |
| --- | --- | --- | --- |
| Drupal recommended project | `ext-gd` | 302 to `/core/install.php` | Same 302 |
| Laravel skeleton | `ext-xml` from Pint | 200 | 200 |
| Copied Laravel application | `ext-xml` from Pint | 200 | 200 |
| Copied Symfony application | `ext-xml` | 200, route parameter `Ada` | Same 200 |
| Roots Bedrock | `ext-simplexml` from ParaTest | 200, WordPress site title | Same 200 |
| Symfony skeleton | `ext-xml` | 200, route parameter `Ada` | Same 200 |
| Symfony webapp | `ext-phar` from phar-io/manifest | 500, Finder string conversion | 200, route parameter `Ada` |

The Symfony webapp also reaches the unsupported
`DOMDocument::schemaValidateSource()` during a Phera cache warmup. Bedrock
used a disposable MySQL server and an installed WordPress site.
Both Laravel projects ran their typed post-autoload callback and Phera's
`artisan package:discover`.

All seven match parsed installed.json and installed.php. Drupal's
`web/core` and Bedrock's `web/wp` and `web/app/mu-plugins` package files
match by path and content. The following differences remain across these
framework installs.

| Generated file | Projects | Cause |
| --- | --- | --- |
| `vendor/pest-plugins.json` | Bedrock | Composer's Pest plugin registers Pest plugins. Phera skips that plugin. |
| `vendor/drupal/DrupalInstalled.php` | Drupal | Composer's scaffold plugin generates a cache version class. Drupal falls back to its own version without it. |
| `vendor/composer/autoload_classmap.php` and `autoload_static.php` | Drupal, three Symfony projects | Drupal's Composer-only cache class adds a map entry. Symfony's one-byte class key is escaped by Phera and emitted literally by Composer. |
| `vendor/composer/include_paths.php` | Drupal | Composer puts `pear/pear_exception` before `pear/console_getopt`; Phera's dependency-weight ordering reverses those two paths. |
| `vendor/bin/*` | Projects with executable dependencies | Phera emits shorter PHP 8.5 and shell proxies. |

Both PHP and Phera produce identical framework class probes for six projects.
The Drupal probe differs only by the Composer-only cache class above.

With Flex auto-scripts active, the basic Symfony and copied Symfony projects
also complete their native console hooks. Put the comparison route in place
before installation so Symfony's cache warmup includes it. The webapp
comparison tree installed with `--no-scripts` serves the same parameterized
route under PHP 8.5. Stock `cache:clear` still blocks the webapp gate.
Symfony console hooks use Phera as required.

### Symfony webapp cache failures

Symfony's translator setup passes an SPL file object to `basename()`.

```php
<?php echo basename(new SplFileInfo('/a/b'));
```

PHP 8.5.12 prints `b`. Phera throws
`Error: Object of class SplFileInfo could not be converted to string`.
An explicit string cast of an SPL file object succeeds in both interpreters.

Using `getBasename()` at that call site in a disposable fixture lets
`cache:clear` reach the XLIFF validator.

```php
<?php
$doc = new DOMDocument();
$doc->loadXML('<r/>');
var_dump($doc->schemaValidateSource(
    '<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r"/></xs:schema>'
));
```

PHP 8.5.12 prints `bool(true)`. Phera throws
`Phera\NotSupportedException: DOMDocument::schemaValidateSource() is not supported by Phera`.

## Fresh dependency resolution and audit

All 25 original projects resolved the same versions as Composer 2.11.2
without their original locks. Composer accepted and installed all 25 locks
from Phera. The seven framework manifests were resolved with the same
platform exclusions and plugins disabled.

| Framework | Packages selected by each tool | Version differences |
| --- | ---: | ---: |
| Drupal recommended project | 68 | 0 |
| Laravel skeleton | 110 | 0 |
| Copied Laravel application | 110 | 0 |
| Copied Symfony application | 61 | 0 |
| Roots Bedrock | 73 | 0 |
| Symfony skeleton 8.1 | 32 | 0 |
| Symfony webapp 8.1 | 128 | 0 |

A Composer-generated lock pinning Symfony HttpFoundation 7.3.0 produced the
same six advisory IDs in both tools. Both exited 1 with active advisories
and exited 0 when all six IDs were configured under `config.audit.ignore`.
Phera's `--no-dev` filtered those advisories when the fixture packages were
in the lock's development section.

## Historical Composer differential results from PR #15

Oracle: Composer 2.11.2 under PHP 8.5.12 on macOS, 2026-11-17. The manifests
come from the release references recorded in each fixture's provenance.json.
Locks were generated by that Composer oracle. The harness restores source files
omitted by release archives when eager autoload files or local repositories need them.

Both tools use `--no-scripts --ignore-platform-reqs -o`; Composer additionally uses
`--no-plugins`. This comparison intentionally separates installation/resolution from
the current Phera runtime's extension coverage and Composer plugin execution.

Each row checks install contents, executable bits, installed.json, class-map lookup
for every discovered class and missing classes, eager-file ordering, PSR mappings,
full resolution without the original lock, Composer lock freshness validation,
and a fresh Composer install from the Phera-generated lock. Deprecations are
captured and compared; they are not suppressed.

| Fixture | Files* | installed.json | Autoloader | Versions | Handoff |
| --- | --- | --- | --- | --- | --- |
| composer/class-map-generator | Same | Same | Same | Same | Pass |
| composer/metadata-minifier | Same | Same | Same | Same | Pass |
| composer/semver | Same | Same | Same | Same | Pass |
| doctrine/collections | Same | Same | Same | Same | Pass |
| guzzlehttp/guzzle | Same | Same | Same | Same | Pass |
| laravel/laravel | Same | Same | Same | Same | Pass |
| league/commonmark | Same | Same | Same | Same | Pass |
| league/csv | Same | Same | Same | Same | Pass |
| league/flysystem | Same | Same | Same | Same | Pass |
| nesbot/carbon | Same | Same | Same | Same | Pass |
| nikic/php-parser | Same | Same | Same | Same | Pass |
| psr/cache | Same | Same | Same | Same | Pass |
| psr/container | Same | Same | Same | Same | Pass |
| psr/http-client | Same | Same | Same | Same | Pass |
| psr/http-factory | Same | Same | Same | Same | Pass |
| psr/http-message | Same | Same | Same | Same | Pass |
| psr/log | Same | Same | Same | Same | Pass |
| ramsey/uuid | Same | Same | Same | Same | Pass |
| slim/slim-skeleton | Same | Same | Same | Same | Pass |
| symfony/console | Same | Same | Same | Same | Pass |
| symfony/filesystem | Same | Same | Same | Same | Pass |
| symfony/finder | Same | Same | Same | Same | Pass |
| symfony/polyfill-mbstring | Same | Same | Same | Same | Pass |
| symfony/skeleton | Same | Same | Same | Same | Pass |
| webmozart/assert | Same | Same | Same | Same | Pass |

*Files excludes the generated files and proxies described below.

All 25 resolve to the same package versions, have equal parsed installed.json
records, pass the PHP autoloader comparison, and can be installed by Composer
from Phera's lock. There are no remaining equal-preference resolution differences
in this matrix. This is evidence for these fixtures, not proof of all Composer behavior.
See the archived [macOS comparison][macos] and [Linux comparison][linux] for the
original records. The current test fetches those 25 pinned manifests and locks
into a temporary directory and checks their upstream source revisions.

## CI cache isolation

A fresh Linux run and the isolated-build-cache run both pass all 25 fixtures.
An intervening CI cache restore left package object directories without their
regular files: Rust's artifact-cache cleanup also pruned comparison data stored
under its target directory. Cargo artifacts now use `target/package-build` in
that job; comparison/source data lives outside the cached build directory.
All comparison assertions remain strict. Store inventories separately detect
missing/changed-size files, links and executable bits and repair incomplete objects
online or report the package missing offline. Same-size content tampering is not
audited on every warm reuse; archive checksums are verified on download.

## Local path packages

`league/flysystem` locks `league/flysystem-local` at `src/Local`. Both installers
create the same relative directory link, including its trailing slash. Focused
local tests also verify live source edits, explicit private mirrors, cached offline
restoration without the source, and removal without deleting source files.

## Generated files

Every fixture has generated-file differences. The data and runtime probes above
are compared separately; byte differences are not called identical.

- `autoload.php`, `composer/autoload_{psr4,namespaces,classmap,files,static,real}.php`:
  Phera generator comments, formatting, deterministic loader suffix and path
  expressions differ. The lookups, PSR directory precedence and eager files are
  compared by real PHP. Empty autoload_files.php may exist where Composer omits it.
- `composer/installed.json`: formatting and key order differ; decoded records are
  equal in every fixture. `composer/installed.php` also has different formatting,
  root reference handling and path expressions. Complete root/alias API parity is
  not covered by the class-lookup probe.
- `composer/platform_check.php`: Phera emits a no-op file with
  `--ignore-platform-reqs`; Composer omits it. Without that option Phera emits PHP
  version/word-size guards and optional extension checks. Exact Composer error
  text and all provider/platform edge cases remain incomplete.
- `composer/LICENSE`: the complete upstream source license is retained; its
  formatting differs from the phar-distributed file.
- `vendor/bin/*`: PHP 8.5 proxies have the Composer globals and are executable,
  but shorter templates and shell wrappers differ. The unit test executes a PHP
  proxy; full PHPUnit isolation, Windows and older-PHP compatibility are unfinished.
- `composer/ClassLoader.php` and `composer/InstalledVersions.php` do not differ:
  they are copied verbatim from upstream Composer, with their MIT license.

## Offline and solver tests

The gate checks all 25 Composer content hashes without network access. Frozen
composer/semver data covers 73 successful normalizations, 26 invalid versions,
141 constraint inputs with 12,408 version matches, 118 satisfies cases and 39
invalid constraints. Synthetic registries cover backtracking, conflicts, cycles,
replace/provide, root conflicts, platform rejection, stability preferences, lowest
versions, branch aliases, inline aliases and offline metadata reuse. Hostile zip
and tar archives, checksum failures, store isolation, no-dev removal, source
refresh, artifact installs and script ordering have focused regressions. Locked
platform checks cover incompatible PHP before vendor mutation, config.platform,
development requirements, extension providers, conflicts and an explicit bypass.

## Readiness gate

This PR remains draft. Local package tests, workspace Clippy, formatting and the
safe-Rust guard pass. A complete local `make ci` was not run: its prerequisite
script creates/grants database accounts, which this task forbids touching on this
machine.
All five hosted gate jobs pass on synchronized code head `c99758c`, including
workspace tests and PDO end to end. See the [hosted CI result](
https://github.com/GijsRommers/Phera/actions/runs/44180505737).
No existing test was weakened, skipped or deleted by this package implementation.

A release CLI smoke test ran a simple `@php` echo script through Phera itself.
The PHP-callable bootstrap then failed because the current runtime does not define
`spl_autoload_register`; a separate script using `file_put_contents` also failed
because that function is absent. These are observed runtime blockers, not passes.
No VM, value or extension implementation was changed to work around them.

The script/IO APIs, binary-proxy edge cases, repository/authentication protocols,
and platform-provider handling listed in the package README are still required
before claiming the entire requested first version is done.

## Measurements

Release code `c99758c86e02e5a38298ebcec3e9097edfc38f70`, macOS arm64, 10 logical CPUs,
Composer 2.11.2 and PHP 8.5.12. These are complete CLI process measurements on a
shared machine, not isolated filesystem microbenchmarks. Seconds are wall time /
child user plus system CPU. Cold caches are cleared per fixture and tool; cold
runs are single observations, warm rows are medians of three. Vendor is removed
before every install. Updates remove the lock and use `--no-install`. Both tools
disable scripts and platform requirements; Composer disables plugins and audits.
Offline uses Phera `--offline` and Composer `COMPOSER_DISABLE_NETWORK=1`.

| Fixture | Operation / cache | Composer wall / CPU | Phera wall / CPU |
| --- | --- | ---: | ---: |
| laravel/laravel | install / cold | 5.797 / 4.304 | 3.958 / 5.316 |
| laravel/laravel | install / warm | 2.013 / 3.916 | 0.879 / 3.191 |
| laravel/laravel | install / warm offline | 1.524 / 3.883 | 0.911 / 3.145 |
| laravel/laravel | update / cold metadata | 1.944 / 0.750 | 9.665 / 2.456 |
| laravel/laravel | update / warm metadata | 1.745 / 0.810 | 7.616 / 2.700 |
| league/commonmark | install / cold | 8.213 / 5.143 | 5.167 / 4.772 |
| league/commonmark | install / warm | 2.195 / 4.591 | 0.710 / 2.716 |
| league/commonmark | install / warm offline | 1.771 / 4.525 | 0.717 / 2.630 |
| league/commonmark | update / cold metadata | 2.113 / 0.731 | 3.959 / 0.926 |
| league/commonmark | update / warm metadata | 1.796 / 0.593 | 2.862 / 0.648 |
| league/csv | install / cold | 6.079 / 3.899 | 4.304 / 3.731 |
| league/csv | install / warm | 1.694 / 3.560 | 0.755 / 1.973 |
| league/csv | install / warm offline | 1.310 / 3.507 | 0.763 / 1.977 |
| league/csv | update / cold metadata | 0.875 / 0.446 | 1.862 / 0.527 |
| league/csv | update / warm metadata | 1.002 / 0.411 | 1.940 / 0.395 |

Warm online installs are 2.24–3.09 times faster in this sample.
The requested tenfold improvement is not reached. Update wall times are slower
than Composer in these observations. Cold network results vary with the registry
and archive hosts. See the [original benchmark records][benchmarks].

[macos]: https://github.com/GijsRommers/Phera/blob/79d628d99532c2b6b7267521cf0b2f801cebfbb0/crates/packages/tests/COMPARISON.json
[linux]: https://github.com/GijsRommers/Phera/blob/79d628d99532c2b6b7267521cf0b2f801cebfbb0/crates/packages/tests/COMPARISON.linux.json
[benchmarks]: https://github.com/GijsRommers/Phera/blob/79d628d99532c2b6b7267521cf0b2f801cebfbb0/crates/packages/tests/BENCHMARKS.json
