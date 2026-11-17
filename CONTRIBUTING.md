# Contributing to Phera

Phera has two hard rules: no `unsafe` in workspace code, and PHP behavior is
verified against real PHP 8.5. Preserve the oracle tests and their expectations.

Install Rust with rustup; `rust-toolchain.toml` selects **1.99.1**, including
rustfmt and Clippy. This matches development and supports edition 2024 and
let-chains. A C compiler is needed for the bundled static PCRE2 build.

Install PHP 8.5 with json, pcre, ctype, pdo_pgsql, pdo_mysql and mbstring, then start
PostgreSQL 17 and MySQL 8.4 using the commands in the
[PDO tests README](crates/extensions/pdo/tests/README.md). Wait for both to accept
connections. Use only the dedicated ports 54329 and 33069; leave port 5432 alone.

```sh
export PHERA_PHP=/absolute/path/to/php
make ci
```

All test suites resolve PHP through the same helper: `PHERA_PHP`, then
`/opt/homebrew/bin/php`, then `php` on `PATH`. For `make ci` the preflight command
uses `PHERA_PHP` or `php` on `PATH`; set `PHERA_PHP` explicitly for exact parity.
`make ci` checks formatting, Clippy, the no-unsafe guard, and prerequisites, then
runs `cargo test --workspace --no-fail-fast` without setting `PHERA_BIN`. Native PDO
tests against both databases remain in this gate. CI uses a PHP wrapper that loads
only the required shared modules even with `-n`; preflight also runs with `-n`.
For installations with shared modules, use
`PHERA_PHP=$PWD/.github/scripts/php-oracle.sh make ci` (optionally set
`PHERA_PHP_REAL` to the actual PHP binary).

CI's separate **PDO end to end (known gaps)** job builds the release CLI, sets
`PHERA_BIN` and runs `cargo test -p phera-ext-pdo --no-fail-fast`. It is informational
until the VM links native class constants and the extension API carries string
exception codes and properties. To run it locally, use `make ci-build`, then set
`PHERA_BIN=$PWD/target/release/phera` for that Cargo command. Run one PDO suite at a
time against shared local databases.

Individual checks are `make fmt-check`, `make lint`, `make safe-rust` and
`make test`. `make test` alone does not provision or verify prerequisites.

If claiming a speed improvement, follow
[Measure inside the process](benchmarks/README.md#measure-inside-the-process).
The separate informational PR benchmark job compares base/head release builds
with identical scripts, five measured runs, medians and `hrtime(true)` inside
the process. It checks output parity; differences below about 10% on shared
runners mean nothing. Results go to the job summary. PR comments would require
additional write permissions, so the workflow keeps its token read-only.
