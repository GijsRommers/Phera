# Native Composer packages

`phera install` reads `composer.lock` without resolving. With no lock it resolves
`composer.json`, writes a Composer lock and installs it. The package crate has no
runtime dependency; the CLI supplies the platform packages reported by its Phera
runtime. Locked installs validate platform requirements before changing vendor.
Installed projects load under ordinary PHP.

```sh
phera install --frozen-lockfile --no-scripts -o
phera install --offline --no-dev
phera install --locked --classmap-authoritative
phera update --no-install
phera add psr/log:^3
phera remove psr/log
phera why psr/log
phera outdated
phera dump-autoload -o
```

Use `--verbose` to see each package's materialization method. `--no-scripts` is
available for controlled installations and differential tests. Platform checks
use Phera's registered capabilities plus `config.platform`. Use
`--ignore-platform-req ext-name` to skip one requirement. The comparison harness
uses `--ignore-platform-reqs` in both tools so their different extension sets
do not affect the package comparison.

## Store and safety

The default store is `$XDG_CACHE_HOME/phera/store`, or
`~/Library/Caches/phera/store` on macOS, or `~/.cache/phera/store` elsewhere.
`PHERA_CACHE_DIR` overrides its complete location. Objects are addressed by SHA256;
package indexes and per-package filesystem locks coordinate reuse. Each index
records file sizes, links and executable bits; incomplete objects fail offline
and are repaired from a verified archive online. Downloads and
materialization run in parallel. PHP declaration scans use all available cores
and cache declaration names by source content.

HTTPS is required unless the project explicitly sets `config.secure-http` to
`false`. Credentials are used only over HTTPS at the original request origin.
Downloads share the runtime's rustls configuration with the ring provider.
Each archive is prepared before the install changes package files in `vendor/`.

Vendor files are copy-on-write clones where available, otherwise private copies.
There is deliberately no hard-link fallback: every root script can modify any
installed package, so private copies protect the global store on filesystems
without reflinks. The installer verifies Composer dist SHA1 checksums when present.

Archives are staged before publication. Paths cannot be absolute or contain
parent traversal; symlinks must stay within the eventual package. Archive limits
are 200,000 entries, 256 MiB per file, 2 GiB unpacked, and 1 MiB for tar long-name/PAX
metadata. Tar metadata is checked before the tar library allocates those buffers.
Device entries, tar hard links, sparse files and symlink chains are rejected.
Project-local path packages use live relative symlinks on Unix by default.
`options.symlink: false` selects a private mirror; `options.relative: false` uses
an absolute link. Online installs also refresh a cached snapshot. Offline installs
can restore that snapshot if the original source is absent. Windows uses mirrors.

## Compatibility and remaining work

Read [tests/DIFFERENCES.md](tests/DIFFERENCES.md) for the measured fixture matrix,
exact generated-file differences, benchmarks and the readiness gate.

- Composer HTTP(S) v2 metadata, static `packages` repositories, inline `package`,
  path and artifact repositories are implemented. Legacy provider/include protocols,
  VCS repositories and complete Composer repository filtering remain unsupported.
- Project `auth.json` and `COMPOSER_AUTH` support host-scoped HTTP Basic and bearer
  credentials. Global Composer authentication files, OAuth-specific helpers and
  forwarding these credentials into git source clones remain unfinished.
- Root pre/post install, update and autoload-dump events run. Shell commands run in
  the project with vendor/bin on PATH; `@php` uses the current Phera binary.
  PHP callable scripts use a generated bootstrap and still need Composer Event
  objects and framework tests. Package events, aliases with arguments and PHP
  `-r` forwarding are unfinished. Composer plugins are listed and skipped.
- The PHP 8.5 binary proxies expose Composer's globals. Older-PHP stream wrappers,
  PHPUnit process-isolation special cases, Windows batch proxies and bash-sourcing
  behavior have not been ported from Composer's proxy generator.
- Vendor/bin configuration and local repository locations currently require
  relative project-local paths.
- Default platform capability reporting is conservative. Complete extension-driver
  and library-version reporting and platform virtual-package providers need more work.

## Verification and licensing

`cargo test -p phera-packages` uses only on-disk fixtures, loopback servers started
inside tests, and the local PHP oracle. Network comparisons require
`PHERA_PACKAGES_NETWORK=1`, `PHERA_BIN`, and `PHERA_COMPOSER`. The network test
fetches the 25 original fixtures by commit and runs the pinned project sources
with scripts and plugins disabled. `PHERA_PACKAGES_COMPARISON_DIR` sets the
location for logs, results and caches outside the checkout.
`PHERA_PACKAGES_INSTALL_ONLY=1` runs the locked-install comparisons on their
own. The registry CI job runs both install and resolution checks.

Composer's ClassLoader.php and InstalledVersions.php are included verbatim under
their MIT license. See [resources/README.md](resources/README.md) for source commits
and licenses, including the composer/semver algorithms ported to Rust.
