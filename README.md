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
> place, and the lexer and parser are just getting started. It can't run PHP yet.

## ✨ Goals

- 🐘 **Framework-driven scope.** If Symfony and Laravel need it, it's in. If nothing modern uses it, it probably isn't.
- 🚀 **High concurrency.** Serve many requests in parallel from a single process.
- 🧱 **Clean layering.** Each stage of the pipeline is its own crate with strict, one-way dependencies.
- 🦀 **Rust all the way down.** Memory safety and predictable performance, no C extensions required.
- 🌐 **Batteries included.** `phera serve` boots an HTTP server straight from the CLI.

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
| `phera-runtime`  | Ties compilation and execution together and owns execution policy    |
| `phera-server`   | HTTP transport on top of the runtime                                 |
| `phera-cli`      | The `phera` binary, for running scripts and `phera serve`            |

Crates planned for later include `io`, `stdlib`, `postgres`, `mysql`, `redis`,
and eventually a `jit`. See [`docs/architecture.md`](docs/architecture.md) for
the details.

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

Rather than chasing PHP's entire `.phpt` suite, Phera runs a hand-picked subset
of it: the tests listed in [`tests/phpt/selected.txt`](tests/phpt/selected.txt).
`tests/fixtures/` holds Phera's own test programs.

## 📜 License

Dual-licensed under MIT or Apache 2.0, at your option.

---

Made with 🦀 and ☕ by **Gijs Rommers**
