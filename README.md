# ⚡ Phera

**A PHP runtime written in Rust, built for Symfony, Laravel, and serious concurrency.**

![status](https://img.shields.io/badge/status-early%20development-orange)
![rust](https://img.shields.io/badge/rust-2024%20edition-b7410e?logo=rust)
![license](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue)

PHP runs a huge share of the web, but its classic execution model starts every
request from scratch and leans on process managers to scale. Phera takes a
different route: a PHP runtime in Rust with a bytecode VM and a built-in HTTP
server designed to handle many requests at once.

Phera is **not** a 1:1 reimplementation of PHP. It doesn't try to reproduce every
legacy quirk, extension, or corner of the language. It targets the PHP that modern
applications actually use, with Symfony and Laravel setting the bar for what has
to work.

> [!WARNING]
> Phera is in early development. It runs real applications, but it is slower
> than PHP today and not ready for production. The numbers below say how far
> it is.

## 📍 Where it stands

Measured in November 2026 against PHP 8.5, the same script or request on both.

| What | State |
| --- | --- |
| Laravel 13 JSON API with Eloquent | Runs. `artisan migrate` and 20 API requests (validation, create, read, update, delete, pagination, relations, a transaction) give PHP's answers on SQLite, MySQL and PostgreSQL |
| Symfony 7.4 with Doctrine | Runs. `doctrine:schema:create` and 22 API requests (routes with parameters, validation, relations, aggregate queries) give PHP's answers on SQLite |
| WordPress | `mysqli` and the engine fixes it needed are in. The end-to-end comparison with PHP has not been repeated since |
| PHP's own test suite (php-src) | 9,136 of the 13,388 core tests pass. Of 4,542 tests for the later extensions (dom, the XML family, openssl, session, curl, intl and others) 1,737 pass |
| Speed of plain scripts | Between about the same CPU time as PHP's interpreter (arrays, arithmetic) and 5.5 times as much (method calls) |
| Speed of a framework request | About 2.1 times PHP's instruction count for one Laravel API request in production mode: 34.5 million instructions against 16.6 million for PHP with opcache, on a plain route. Phera boots the framework once per worker and answers each request from a copy of the booted application ([below](#-boot-once-fork-every-request)). A route that reads the database costs 138 million against 27 million, 5.0 times, and a Symfony and Doctrine route with 49 statements 301 million against 30 million, 10 times. Under load, measured before this way of running existed, PHP-FPM behind nginx served 8,288 requests per second on a plain route and Phera 228; that test has not been repeated yet |

Phera validates union and intersection declarations during compilation and
checks inherited property types when linking classes. Ordinary typed
parameters with a literal `null` default accept `null` and emit PHP's
implicit-nullability deprecation.
Typed property references require the same conversion for every property
holding the reference.
During cycle collection, a WeakMap entry keeps its value reachable only
while the map and key are reachable.
Closure captures, static values and bound receivers participate in
cycle collection.
The compiler diagnoses ambiguous ternary chains, invalid loop operands
and misplaced namespace declarations. Syntax-only tokenization accepts
those constructs.

## 🧰 The `phera` command

| Command | What it does |
| --- | --- |
| `phera file.php` | Runs a script |
| `phera serve public` | Serves a directory over HTTP: one process, one worker thread per core, many requests per thread |
| `phera new shop` | Starts a project: a small JSON application, or Symfony's or Laravel's official one |
| `phera check src` | Type-checks PHP with PHPStan's levels and identifiers; reads a PHPStan baseline |
| `phera lint src` | Finds unused code and suspicious constructs; `--fix` applies the safe fixes |
| `phera install` | Installs dependencies from `composer.lock` or resolves `composer.json` |
| `phera upgrade` | Updates Phera. Use `--check` to see the available version or `--version X` to install a specific release |
| `phera --dump file.php` | Prints the compiled bytecode |

`phera <command> --help` lists the options. The built-in package manager reads
Composer manifests and locks. The formatter, test runner and step debugger
are still in development.

### Installing dependencies

The package manager is in development. Try
`phera install --locked --no-scripts` to install the versions in an existing
Composer lock and generate `vendor/autoload.php`. Use `--offline` to install
from a warm cache, `--no-dev` to omit development dependencies, or
`-a` to generate an authoritative class map. See
[package compatibility](crates/packages/README.md) for the supported commands
and current gaps.

When `config.allow-plugins` enables Symfony Runtime, the installer also
generates its runtime bootstrap from the installed package. Approved Drupal
and WordPress installers use the project's custom package paths.
Drupal's scaffold hook copies missing entry points and other mapped files.
The approved Symfony Flex hook can restore missing recipe files and create
the initial bundle list and dotenv defaults.
Use `--no-plugins` to skip native plugin hooks.
In a terminal Phera can save a Composer plugin trust decision to composer.json.
Use `--no-interaction` to require a decision already present in the project.
Symfony console hooks run on Phera. The stock Symfony webapp's cache warmup
still needs runtime fixes. See the
[webapp test results](crates/packages/tests/DIFFERENCES.md#symfony-webapp-cache-failures).

`phera new` fetches official framework templates through the package manager.
Composer is the fallback when the native installer refuses a template or its
dependency requirements. Use `--no-install` to fetch just the starter files.

Use `phera update vendor/package` to update selected dependencies, or
`--dry-run` to see what would change. `-W` also unlocks their dependencies.
`phera update --lock` refreshes the content hash without resolving.
`phera add` (also `phera require`) and `phera remove` accept several packages.

`phera audit` checks installed packages against Packagist security advisories.
Use `--locked` for composer.lock, `--no-dev` for production packages, and
`--format json` for automation. Put reviewed advisory IDs in
`config.audit.ignore` in composer.json. Installs and updates print a
short advisory summary online unless you pass `--no-audit`.

Projects can use Packagist, Composer, Git, path and artifact repositories.
Git over HTTPS and SSH uses the system client. Put repository credentials in
`auth.json` or `COMPOSER_AUTH`, keyed by the host that receives them.

Use `phera show`, `phera licenses`, and `phera validate` to inspect an
installation. `phera why-not vendor/package version` explains locked
constraints, and `phera outdated --all` also includes transitive dependencies.
`phera run-script name` runs a composer.json script. `phera run name` accepts
the same script name when it is neither an existing file nor a PHP path.
`phera exec name` runs a project binary from vendor/bin.
The installer reports package progress in a terminal and prints elapsed time
when it finishes.

### Serving a site

Run `phera serve public` to serve a document root. PHP diagnostics, including
fatal backtraces, go to the server log by default. Use `phera serve --dev public`
to display diagnostics in responses and check PHP files on each request during
local development.

The web SAPI queues stderr diagnostics and `error_log()` messages. When the
queue fills or a record exceeds 64 KiB, it drops the record so the request can
finish. Explicit file destinations keep their file write behavior.

`--request-read-timeout SECONDS` sets the initial time budget for receiving
headers and the body. It defaults to 60 seconds and gains one second for every
16 KiB of body data received. If the budget runs out, the server returns HTTP 408
and closes the connection.

`phera serve` caches compiled PHP files and rechecks their paths and timestamps
every two seconds by default. Set `-d opcache.revalidate_freq=0` to check on
each request with production diagnostics, or `-d opcache.validate_timestamps=0`
to keep cached code until the server restarts. `opcache.revalidate_path=1`
checks the include search path on each lookup. `--dev` enables timestamp
validation and sets the interval to zero.

Request teardown releases circular arrays and objects held by their class's
static properties or cached constant values.

Use `phera serve --preload public` or set `preload = true` under `[server]`
in the project's `phera.toml`. At startup Phera compiles the PHP files in
Composer's generated class map and files autoload list. The start line gives
the count and elapsed time. PHP still runs each file when the application
includes it. The Zend-specific `opcache.preload` directive is unsupported.

## 🧩 Extensions

Built in, each checked call by call against real PHP: core, standard, ctype,
[curl](crates/extensions/curl/README.md) (HTTP and HTTPS, including HTTP/2,
connection reuse, and the easy and multi interfaces), date, dom (`DOMDocument`,
`DOMNode`, `DOMElement`, `DOMXPath`), filter, hash, intl, json, mbstring, openssl, pcre
(`MARK` in match arrays, including Symfony's compiled route patterns),
PDO (SQLite, MySQL, PostgreSQL, with TLS), random, redis, reflection, serialize,
session, sodium, SPL, sqlite3, streams, tokenizer, zlib, bcmath, gmp, Unix process
functions, and POSIX queries.

Register an application stream wrapper with
`stream_wrapper_register('virtual', VirtualFile::class)`. The registration
belongs to the current request. `fopen()`, `file_get_contents()`, and
`file_put_contents()` can use paths such as `virtual://report`. An included wrapper URL is resolved
by the application during the current request.

Wrapper and filter registrations made during a snapshot boot carry over to
each fork. Open application wrapper handles and attached filters must be
created after the snapshot.

Use `php://filter` or `stream_filter_append()` to transform a stream.
Register PHP filters with `stream_filter_register()` and combine them with
built-in filters on native streams or application wrappers. Phera limits
chains to 16 stages and intermediate buckets to 8 MiB.

Stream sockets use the worker's event loop for TCP, UDP and Unix connections.
PHP programs can open a client or listener, wait for readiness with
`stream_select()`, and use the regular read, write, and timeout functions.
TCP clients can start TLS when connecting or upgrade an existing connection.
Listeners can accept TLS directly or upgrade an accepted connection.
By default clients verify the server's certificate and host name.
A live socket held by the boot prevents `Phera\snapshot()` from creating
a snapshot. Persistent sockets created during boot connect on first use
in each fork. `pfsockopen()` and `STREAM_CLIENT_PERSISTENT` can reuse a
clean idle connection on the same worker.
`fopen()` and `file_get_contents()` support buffered HTTP and HTTPS
responses. `get_headers()` fetches the response headers, and
`http_get_last_response_headers()` returns the latest headers in the
current request. HTTP calls also populate the caller's legacy `$http_response_header` variable.
URL credentials use Basic authentication and stay on their origin. Stream contexts can store
default options for these calls.

PHP host, DNS and MX lookups leave the worker available while waiting.
Service and protocol lookups use the system tables, and
`net_get_interfaces()` exposes local interface addresses, including the system's IPv6 scope
bits on Apple. `getmxrr()` and `dns_get_mx()` return false when the answer contains no mail exchanger.

Modern DOM supports CSS selectors, live class tokens, and `innerHTML`.
Legacy `DOMDocument` supports local XSD validation for common content models,
including Symfony's service configuration. Complex type derivation and XSD
identity constraints remain unsupported.

- Documents and elements include `Dom\Document`, `Dom\HTMLDocument`,
  `Dom\XMLDocument`, `Dom\Element`, `Dom\HTMLElement`, `Dom\Attr`.
- Other nodes include `Dom\Node`, `Dom\CharacterData`, `Dom\Text`,
  `Dom\CDATASection`, `Dom\Comment`, `Dom\ProcessingInstruction`,
  `Dom\DocumentType`, `Dom\DocumentFragment`, `Dom\Entity`,
  `Dom\EntityReference`, `Dom\Notation`.
- Collections and queries include `Dom\NodeList`, `Dom\NamedNodeMap`,
  `Dom\DtdNamedNodeMap`, `Dom\HTMLCollection`, `Dom\TokenList`, `Dom\XPath`.
- `Dom\ParentNode` and `Dom\ChildNode` expose their shared operations.

The XML extensions include `XMLWriter` for memory and URI output,
`SimpleXMLElement` and `SimpleXMLIterator` for XML and DOM interoperability,
`XMLReader` for forward traversal, and `XMLParser` for XML events.
XML inputs use bounded entity expansion and keep external entity declarations inert.
XMLReader schema validation remains incomplete.

Named arguments to built-ins use PHP’s declared defaults for skipped parameters.
For example, `json_decode($json, flags: JSON_THROW_ON_ERROR)` uses the default
depth. Reflection reads the same defaults, and native errors include the
function or method that raised them in the exception trace.

`min()`, `max()`, and loose array searches use native object comparisons,
including comparisons inside nested arrays. `range()` accepts Stringable
endpoints in weak mode.
`array_multisort()` sorts parallel arrays by their columns and accepts sort
flags as literals. Writable arguments receive the sorted arrays.

Unix scripts can launch programs with `shell_exec()`, backticks, and the process
functions. Backticks emit PHP 8.5's compile-time deprecation. On Unix, `mail()`
and `mb_send_mail()` pipe the message to the command in `sendmail_path`.
`phera serve --no-process public` disables program launches for web requests,
including `mail()` and `mb_send_mail()`.
The standard library retains PHP’s deprecated `utf8_encode()` Latin-1 conversion.

BCMath provides arbitrary-precision decimal functions and the immutable
`BcMath\Number` class. Set the default procedural scale with `bcmath.scale`
or `bcscale()`.

GMP provides arbitrary-precision integer functions, binary import and export,
and the `GMP` class with arithmetic, bitwise operators, and scalar casts.

zlib provides string compression, incremental deflate and inflate contexts,
gzip file functions, and the `compress.zlib://` wrapper. The
`zlib.output_compression` directive stores its value. Use `ob_gzhandler` for
PHP output-buffer compression or `phera serve --compress` for server compression.

The `mysqli` and `pgsql` extensions share PDO's async connections, TLS and
connection pool. For mysqli single statements, `MYSQLI_USE_RESULT` fetches one
row at a time, including in `foreach`. PostgreSQL supports the `pg_exec` alias
and other PHP 8.5 aliases for supported operations. `pg_copy_from()` accepts
arrays and traversable row sources. PostgreSQL large objects can be opened,
read, written, sought, truncated and unlinked in the current transaction.
`pg_lo_import()` and `pg_lo_export()` transfer local files in bounded chunks.
Native function return types are visible through reflection. Compatibility
testing is in progress. MySQL LOCAL INFILE is disabled.

Fileinfo is built in. Use `finfo` or `mime_content_type()` to identify common
uploads from their contents. Fileinfo uses a bounded Rust classifier for images,
documents, archives, media, fonts, and executables. It provides the MIME type,
charset, known extensions, and a description. Custom magic databases and
libmagic's complete signature catalog are outside this implementation.

GD provides image buffers, PNG, JPEG, GIF, BMP and WebP codecs, drawing,
thumbnail operations, filters, and the five built-in fonts. The same build
includes `getimagesize()` and `exif_imagetype()`. Compatibility limits are
listed in the [GD coverage notes](crates/extensions/gd/README.md).

The sockets extension supports TCP, UDP and Unix sockets, including
descriptor passing with Unix socket messages. A `Socket` and an exported
stream share the same connection. Some platform-specific options, resolver
hints and out-of-band operations remain incomplete.

Not started: zip.

### Database connection pool

MySQL and PostgreSQL sockets stay on the worker that opens them.
`PHERA_PDO_POOL_SIZE` (20 by default) limits pooled sockets to each database
across the process. An idle worker can close one of its sockets when
another worker is waiting for a turn. `PHERA_PDO_POOL_IDLE_SECONDS` (60),
`PHERA_PDO_POOL_LIFETIME_SECONDS` (1800) and
`PHERA_PDO_POOL_WAIT_SECONDS` (30) control the pool's timeouts.
`PHERA_PDO_IO_THREADS` is accepted for compatibility and has no effect.
Unpooled connections, including PDO with `PHERA_ATTR_POOL => false` and
nonpersistent mysqli and pgsql connections opened by a request, are outside
the pool limit. Each fork reconnects a boot connection on first use and
replays the boot's session changes. The fork can borrow a pooled socket for
a nonpersistent mysqli or pgsql connection opened during boot.

PDO placeholder parsing follows each database's quoting and comment rules.
The [PDO coverage notes](crates/extensions/pdo/README.md#differences-from-php-85)
describe two PostgreSQL emulation differences from PHP 8.5.

### Sodium

Phera implements PHP's `sodium` module in Rust. It supports NaCl secret boxes
and sealed boxes, Ed25519 signatures, X25519 and Ristretto255, ChaCha20-Poly1305
and AES-256-GCM, BLAKE2b (including streaming) and SipHash, XChaCha20-Poly1305
secretstream, key derivation and exchange, Argon2 and scrypt passwords, binary
encodings, and ISO/IEC 7816-4 padding. The six AEGIS-128L and AEGIS-256 functions are
absent pending a published audit of a pure Rust backend. Ed25519 signing
rejects secret keys whose embedded public half cannot be decoded as a point.

### intl

The intl extension includes ICU4X normalization, grapheme operations, IDNA,
locale queries, number formatting including significant digits and English
spellout and ordinals, date formatting including relative days and skeletons,
collation, list and message formatting, transliteration with category-filtered
accent removal, break iteration, Unicode character names, UTF and single-byte
conversion, resource bundles, and calendar and timezone objects. Character-name
lookup and the standalone hour patterns covered by the oracle follow PHP
8.5.13 with ICU 78.2. See the
[intl coverage notes](crates/extensions/intl/README.md) for the APIs that still
differ from PHP.

## ✨ Goals

- 🐘 **Framework-driven scope.** Symfony, Laravel and WordPress set the bar for what has to work. If nothing modern uses it, it probably isn't in.
- 🚀 **High concurrency.** Serve many requests in parallel from a single process.
- 🧱 **Clean layering.** Each stage of the pipeline is its own crate with strict, one-way dependencies.
- 🦀 **Safe Rust.** No `unsafe` in Phera's own code, and no C libraries for parsing what comes from the network.
- 🌐 **Batteries included.** `phera serve` is the web server; no PHP-FPM and no separate server in front.

### What's left out on purpose

Deprecated parts of PHP that nothing modern depends on are not built. Using one
is a clear error (`Phera\NotSupportedException` or a compile error), never a
silently different answer:

- `${expr}` string interpolation, deprecated since PHP 8.2 (`"{$expr}"` works)
- The `(real)` cast, removed in PHP 8
- Short `<?` and ASP `<%` open tags
- The other items listed in [`crates/source/data/deprecations.tsv`](crates/source/data/deprecations.tsv)

Why things are the way they are is written down in
[`docs/decisions/`](docs/decisions/README.md).

## 🏗️ Architecture

Source code flows through a classic compiler pipeline:

```
PHP source ─▶ lexer ─▶ parser ─▶ AST ─▶ compiler ─▶ bytecode ─▶ VM ─▶ runtime ─▶ server
```

| Crate            | Role                                                                 |
| ---------------- | -------------------------------------------------------------------- |
| `phera-source`   | Source files and spans, shared by the frontend                       |
| `phera-lexer`    | Turns PHP source into tokens (knows nothing about the parser or AST) |
| `phera-parser`   | Builds the AST from tokens, with structured diagnostics              |
| `phera-ast`      | The syntax tree definitions                                          |
| `phera-compiler` | Lowers the AST to bytecode                                           |
| `phera-bytecode` | The instruction format, independent of PHP parsing                   |
| `phera-vm`       | Executes bytecode, with no dependency on the frontend                |
| [`phera-dotenv`](crates/dotenv/README.md) | Native environment files, syntax and startup options |
| `phera-runtime`  | Ties compilation and execution together and owns execution policy    |
| `phera-server`   | HTTP transport on top of the runtime                                 |
| `phera-cli`      | The `phera` binary, for running scripts and `phera serve`            |

The extensions live under `crates/extensions/`, behind `phera-extension-api`.
See [`docs/architecture.md`](docs/architecture.md) for the design and
[`docs/decisions/`](docs/decisions/README.md) for the decisions behind it.

## Install

Once the first release is published, install Phera on macOS or Linux with
glibc 2.35 or newer (arm64 or x86_64) with:

```sh
curl -fsSL https://raw.githubusercontent.com/GijsRommers/Phera/main/install.sh | sh
```

It verifies the archive and installs Phera in `~/.phera/bin`. Set
`PHERA_INSTALL` to choose another install directory. To pick a version, set
`PHERA_VERSION` or pipe to `sh -s -- v0.1.0`. The installer prints the
`PATH` line when needed.

Run `phera upgrade` to update a writable installation. Use your package
manager to update a package-managed copy. Phera keeps its previous binary
as `phera.old` next to the upgraded one.

An HTTPS mirror can set `PHERA_RELEASE_BASE_URL` for both commands. Serve
`latest` as JSON with a `tag_name` field and put the archives and
`SHA256SUMS` under `download/<tag>/`, for example `download/v0.1.0/`.
The upgrader expects the mirror to serve these files directly.

## ⏩ Boot once, fork every request

`phera serve` can boot an application once per worker thread, before the
first connection, and start every request as a copy of that booted state.
Laravel and Symfony applications get this with no change:

- **Laravel** (a project with `artisan` and `bootstrap/app.php`): Phera runs
  its own entry script for `public/index.php`, which bootstraps the
  application and its service providers, then handles the request.
- **Symfony** with `symfony/runtime`: Phera sets `APP_RUNTIME` to its own
  runtime class (unless something else chose one), which boots the kernel
  and then handles the request.
- **Any other application** marks the point itself:

  ```php
  $kernel = new Kernel('prod', false);
  $kernel->boot();                 // once per worker, at server start
  Phera\snapshot();                // true in every request; false on the command line
  $kernel->handle(Request::createFromGlobals())->send();   // per request
  ```

  The first start serves such a script from the top until it reaches the
  call; later starts boot it before the first connection.

The boot runs without a request: `$_GET`, `$_POST`, `$_COOKIE` and `$_FILES`
are empty and `$_SERVER` holds the environment and the script's paths. Each
request gets its own copy of every object, static property and global, so
nothing one request does reaches another. A database or Redis connection
made in the boot connects again on first use in each request, from the pool,
with what the boot set on its session (`SET`, `USE`, the charset) replayed;
an open file is opened again at the same position; DOM documents, generators,
curl handles and temporary files are copied; a `GdImage` copies its pixels,
palette and rendering options, and a `finfo` object its flags. What cannot be duplicated (an
open transaction, a lock, a suspended Fiber, a pipe to a process, a directory
handle, an open gzip file, an `SQLite3` object, a prepared statement) makes
`Phera\snapshot()` throw `Phera\SnapshotException`, naming the class and
where it was found; the worker then serves the application from the top.
A changed file boots the worker again in the background, following
`opcache.validate_timestamps` and `opcache.revalidate_freq`. The boot's
objects are never destroyed; each request's copies are destroyed at its end,
as PHP-FPM destroys a request's objects. Snapshots are the only mode and
cannot be turned off; a request that has to run from the top (its boot was
refused) is logged as a defect and counted in the `--health` answer's
`Phera-From-The-Top` header.

A Laravel API request took 525 million instructions run from the top, and
102 million as a fork (PHP 8.5 with opcache: 48 million). The design is in
[`docs/architecture.md`](docs/architecture.md) section 5.4, the decisions
in [`docs/decisions/`](docs/decisions/).

## 🛠️ Building

You'll need the Rust toolchain named in `rust-toolchain.toml` (rustup installs
it by itself) and a C compiler for the bundled PCRE2.

```sh
git clone <repo-url> phera
cd phera
cargo build
cargo test
```

The `phera` binary ends up in `target/debug/phera`; `cargo build --release`
builds the fast one. [`CONTRIBUTING.md`](CONTRIBUTING.md) describes the full
check, which compares every built-in function with real PHP 8.5.

## 🧪 Testing

Phera runs the tests of PHP's own `.phpt` suite that it passes: those listed in
[`tests/phpt/selected.txt`](tests/phpt/selected.txt), copied from php-src into
`tests/upstream/`. [`tests/phpt/sweep.sh`](tests/phpt/sweep.sh) runs whole
directories of php-src with real PHP and with Phera and adds every test that
passes on both; [`tests/upstream/README.md`](tests/upstream/README.md) shows how
far that is. `tests/fixtures/` holds Phera's own `.phpt` test programs.

| Command | What it does |
| --- | --- |
| `make test` | All Rust tests, including the lexer comparison below |
| `make phpt` | Runs every selected test and fixture through the `phera` binary and checks the output |
| `make phpt-lexer` | Lexes each test's code with Phera and with PHP's `token_get_all`, and compares them |

Narrow either runner with `FILTER=<part of a path>`. The lexer comparison needs
a `php` binary and skips itself without one.

To check the tests themselves against real PHP, run them with PHP instead:

```sh
PHERA_PHPT_BIN="php -n -d fatal_error_backtraces=0 -d display_errors=1" make phpt
```

## 📜 License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution you intentionally submit
for inclusion in Phera, as defined in the Apache 2.0 license, is dual-licensed as
above, without any additional terms or conditions.

---

Made with 🦀 and ☕ by **Gijs Rommers**
