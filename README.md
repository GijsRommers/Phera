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
| Symfony 7.4 with Doctrine | Runs. `doctrine:schema:create` and 22 API requests (routes with parameters, validation, relations, aggregate queries) give PHP's answers on SQLite. One known defect: under a long run of requests on one worker a request can hang until the time limit; it is being fixed |
| WordPress | `mysqli` and the engine fixes it needed are in. The end-to-end comparison with PHP has not been repeated since |
| PHP's own test suite (php-src) | 9,052 of the 13,388 core tests pass. Of 4,542 tests for the later extensions (dom, openssl, session, curl, intl and others) 1,436 pass |
| Speed of plain scripts | Between about the same CPU time as PHP's interpreter (arrays, arithmetic) and 5.5 times as much (method calls) |
| Speed of a framework request | About 15 times PHP's CPU for one Laravel API request in production mode (249 million instructions against 16.6 million for PHP with opcache). Under load PHP-FPM behind nginx served 8,288 requests per second on a plain route and Phera 228. Phera boots the framework again for every request; booting once per worker and answering each request from a copy of the booted application is in review and brings the same request to 63 million instructions |

## 🧰 The `phera` command

| Command | What it does |
| --- | --- |
| `phera file.php` | Runs a script |
| `phera serve public` | Serves a directory over HTTP: one process, one worker thread per core, many requests per thread |
| `phera new shop` | Starts a project: a small JSON application, or Symfony's or Laravel's official one |
| `phera check src` | Type-checks PHP with PHPStan's levels and identifiers; reads a PHPStan baseline |
| `phera lint src` | Finds unused code and suspicious constructs; `--fix` applies the safe fixes |
| `phera upgrade` | Updates Phera. Use `--check` to see the available version or `--version X` to install a specific release |
| `phera --dump file.php` | Prints the compiled bytecode |

`phera <command> --help` lists the options. There is no formatter and no test
runner yet. A package manager (`phera install`) and a step debugger are being
built; until then packages are installed with Composer.

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

### Step debugging

Phera speaks DBGp, the protocol PHP IDEs use for Xdebug. Start the IDE listener
and run `phera run --debug script.php` or `phera serve --debug public`.

For request-triggered debugging, set `xdebug.mode=debug` using `-d` or
`XDEBUG_MODE=debug`, then send `XDEBUG_TRIGGER=1` or `XDEBUG_SESSION=1` as a
cookie, query parameter, POST form field, or environment variable. The default
destination is `127.0.0.1:9003`. Set `xdebug.client_host` and `xdebug.client_port`
with `-d`, or use `XDEBUG_CONFIG="client_host=127.0.0.1 client_port=9003"`.
You can also set this in your project's `phera.toml`:

```toml
[debug]
enabled = true
start_with_request = "trigger"
client_host = "127.0.0.1"
client_port = 9003
```

The IDE can inspect and change data and evaluate PHP in a stopped request.
Phera connects only to the configured IDE address. A server enabling
debugging on a non-loopback listen address needs `--allow-remote-debug`.
One worker can serve other requests while PHP waits for an IDE command.

PHP exposes the active settings through `ini_get('xdebug.mode')` and the
other `xdebug.*` directives above. PhpStorm's validator checks for the
Xdebug PHP extension. Use **Start Listening for PHP Debug Connections**
and a breakpoint to verify a Phera session.

#### PhpStorm

1. Open the PHP project in PhpStorm.
2. Go to **Settings → PHP → Debug**.
3. Confirm the Xdebug port includes `9003`.
4. Select **Run → Start Listening for PHP Debug Connections**.
5. Set a breakpoint on an executable line.
6. Run `phera run --debug script.php` from the project directory.
7. For HTTP, run `phera serve --debug public` and load the relevant URL.
8. If the IDE prompts for a mapping, map the server path to the project file.
9. Use **Variables**, **Step Into**, **Step Over**, and **Step Out** at the stop.
10. Resume to let the script finish or the request return its response.

#### VS Code

1. Install the **PHP Debug** extension from Xdebug.
2. Open the PHP project in VS Code.
3. In **Run and Debug**, create a PHP launch configuration.
4. Choose **Listen for Xdebug**, with `"port": 9003`.
5. Start the listener with **F5**.
6. Set a breakpoint on an executable line in the PHP file.
7. Run `phera run --debug script.php` in a separate terminal.
8. For HTTP, run `phera serve --debug public` and load the relevant URL.
9. Expand **Variables** and use **Watch** to evaluate an expression.
10. Step or continue from the **Run and Debug** toolbar.

## 🧩 Extensions

Built in, each checked call by call against real PHP: core, standard, ctype,
[curl](crates/extensions/curl/README.md) (HTTP and HTTPS, including HTTP/2,
connection reuse, and the easy and multi interfaces), date, dom (`DOMDocument`,
`DOMNode`, `DOMElement`, `DOMXPath`), filter, hash, intl, json, mbstring, openssl, pcre
(`MARK` in match arrays, including Symfony's compiled route patterns),
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
