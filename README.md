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
| Symfony 7.4 | A skeleton route runs. An application with Doctrine does not yet: it needs destructors that run at the end of a statement, which is being built |
| WordPress | Compatibility testing in progress for the new `mysqli` extension |
| PHP's own test suite (php-src, 13,388 tests) | 8,704 pass |
| Speed of plain scripts | Between about the same CPU time as PHP's interpreter (arrays, arithmetic) and 5.5 times as much (method calls) |
| Speed of a framework request | Roughly 10 times slower than PHP with opcache (one Laravel API request: about 150 ms against 9 to 17 ms). Phera links every class again for each request; sharing linked code between requests, forking a booted application and a worker mode with FrankenPHP's interface are designed ([architecture, 5.4](docs/architecture.md)) and not built. Measured CPU per request for a Laravel route with two MySQL queries: 34 million cycles on PHP with opcache, 369 million on Phera |

## 🧰 The `phera` command

| Command | What it does |
| --- | --- |
| `phera file.php` | Runs a script |
| `phera serve public` | Serves a directory over HTTP: one process, one worker thread per core, many requests per thread |
| `phera new shop` | Starts a project: a small JSON application, or Symfony's or Laravel's official one |
| `phera check src` | Type-checks PHP and PHPDoc at levels 0–10 (`--level max`) and reads PHPStan baselines |
| `phera lint src` | Finds unused code and suspicious constructs; `--fix` applies the safe fixes |
| `phera --dump file.php` | Prints the compiled bytecode |

The checker recognizes Doctrine ORM mapping attributes as property reads
and writes, including generated identifiers and associations. It also
uses entity class strings to infer Doctrine repository, find, and reference
results. It recognizes Doctrine lifecycle callbacks and Symfony service injection,
command, route, listener, and scheduled-task attributes.

`phera <command> --help` lists the options. There is no formatter and no test
runner yet.

### Serving a site

Run `phera serve public` to serve a document root. PHP diagnostics, including
fatal backtraces, go to the server log by default. Use `phera serve --dev public`
to display diagnostics in responses and check PHP files on each request during
local development.

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
`DOMNode`, `DOMElement`, `DOMXPath`), filter, hash, intl, json, mbstring, openssl, pcre,
PDO (SQLite, MySQL, PostgreSQL, with TLS), random, redis, reflection, serialize,
session, sodium, SPL, sqlite3, streams, tokenizer, zlib, bcmath, gmp, Unix process
functions, and POSIX queries.

Modern DOM supports CSS selectors, live class tokens, and `innerHTML`.

- Documents and elements include `Dom\Document`, `Dom\HTMLDocument`,
  `Dom\XMLDocument`, `Dom\Element`, `Dom\HTMLElement`, `Dom\Attr`.
- Other nodes include `Dom\Node`, `Dom\CharacterData`, `Dom\Text`,
  `Dom\CDATASection`, `Dom\Comment`, `Dom\ProcessingInstruction`,
  `Dom\DocumentType`, `Dom\DocumentFragment`, `Dom\Entity`,
  `Dom\EntityReference`, `Dom\Notation`.
- Collections and queries include `Dom\NodeList`, `Dom\NamedNodeMap`,
  `Dom\DtdNamedNodeMap`, `Dom\HTMLCollection`, `Dom\TokenList`, `Dom\XPath`.
- `Dom\ParentNode` and `Dom\ChildNode` expose their shared operations.

Named arguments to built-ins use PHP’s declared defaults for skipped parameters.
For example, `json_decode($json, flags: JSON_THROW_ON_ERROR)` uses the default
depth. Reflection reads the same defaults, and native errors include the
function or method that raised them in the exception trace.

Unix scripts can launch programs with `shell_exec()`, backticks, and the process
functions. Backticks emit PHP 8.5's compile-time deprecation. To disable program
launching for web requests, run `phera serve --no-process public`.

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

In progress: SimpleXML, XMLReader, XMLWriter.
Not started: gd, fileinfo, zip, sockets.

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
locale queries, number and date formatting, collation, list and message
formatting, transliteration, break iteration, resource bundles, and calendar
and timezone objects. See the [intl coverage notes](crates/extensions/intl/README.md) for
the APIs that still differ from PHP.

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
