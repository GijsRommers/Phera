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
> Phera is in its earliest stages. The workspace and crate boundaries are in
> place and the lexer is complete, but the parser is just getting started. It
> can't run PHP yet.

## ✨ Goals

- 🐘 **Framework-driven scope.** If Symfony and Laravel need it, it's in. If nothing modern uses it, it probably isn't.
- 🚀 **High concurrency.** Serve many requests in parallel from a single process.
- 🧱 **Clean layering.** Each stage of the pipeline is its own crate with strict, one-way dependencies.
- 🦀 **Rust all the way down.** Memory safety and predictable performance, no C extensions required.
- 🌐 **Batteries included.** `phera serve` boots an HTTP server straight from the CLI.

### What's left out on purpose

The lexer handles everything PHP 8.5 can lex, with a few deliberate exceptions
that nothing modern depends on. Each is a clear error, not a silent gap:

- Backtick shell execution (`` `ls` ``)
- `${expr}` string interpolation, deprecated since PHP 8.2 (`"{$expr}"` works)
- The `(real)` cast, removed in PHP 8
- Short `<?` and ASP `<%` open tags

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

Crates planned for later include `io`, `stdlib`, `postgres`, `mysql`, `redis`,
and eventually a `jit`. See [`docs/architecture.md`](docs/architecture.md) for
the details.

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
nothing one request does reaches another. What cannot be copied (an open
file or socket, a started session, a database transaction) makes
`Phera\snapshot()` throw `Phera\SnapshotException`, naming the class and
where it was found; the worker then serves the application from the top.
A changed file boots the worker again in the background, following
`opcache.validate_timestamps` and `opcache.revalidate_freq`. The boot's
objects are never destroyed; each request's copies are destroyed at its end,
as PHP-FPM destroys a request's objects. `--no-snapshot` turns all of it off.

A Laravel API request took 525 million instructions run from the top, and
102 million as a fork (PHP 8.5 with opcache: 48 million). The design is in
[`docs/architecture.md`](docs/architecture.md) section 5.4, the decisions
in [`docs/decisions/`](docs/decisions/).

## 🛠️ Building

You'll need a recent stable Rust toolchain with 2024 edition support (1.85+).

```sh
git clone <repo-url> phera
cd phera
cargo build
cargo test
```

The `phera` binary ends up in `target/debug/phera`.

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

`make phpt` isn't part of `make test` yet, since Phera can't run PHP. Narrow
either runner with `FILTER=<part of a path>`.

The lexer comparison needs a `php` binary and skips itself without one. Its
token stream matches `token_get_all` exactly, including inside interpolated
strings and heredocs. Tests that use syntax Phera rejects on purpose, or whose
code Phera rejects where PHP expects a parse error too, are counted as not
relevant rather than failed. Point it at a php-src checkout with
`make phpt-lexer PHP_SRC=/path/to/php-src` to run it over every test there; the
lexer passes all of them.

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
