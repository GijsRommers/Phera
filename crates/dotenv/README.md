# Native environment files

`phera file.php`, `phera run file.php` and `phera serve` load dotenv files before PHP starts.
For example, these two files make `phera app.php` print `localhost:8080`.

```dotenv
# .env
HOST=localhost
ADDRESS="${HOST}:${PORT:-8080}"
```

```php
<?php
echo getenv('ADDRESS');
```

## Which files load

A script uses its containing directory. The server starts from its document root (the containing
directory when serving a single script). If that root has a `composer.json`, it uses the root.
Otherwise it uses the immediate parent if the parent has a `composer.json`, or the root itself.

The last file defining a name wins. A variable inherited from the process always wins, including an
empty string. Expansion sees the winning value when the assignment is read.

1. `.env`
2. `.env.local`
3. `.env.<APP_ENV>`
4. `.env.<APP_ENV>.local`

Phera selects the profile once after step 2, using the inherited `APP_ENV` first, then the merged
files. An absent or empty profile skips steps 3 and 4. The suffix must fit in a single filename.
Missing automatic files are optional. A malformed selected file stops startup before a script runs
or a server binds its socket. Restart the server to pick up edits.

```sh
phera --env-file config/base.env --env-file config/override.env app.php
phera run --no-env-file app.php
phera serve --env-file config/server.env public
```

Repeated `--env-file` options replace automatic discovery and load the required paths in command
line order. Relative explicit paths start at the working directory. `--no-env-file` disables file
loading. The two options are mutually exclusive. Put script options before the script name.
Everything following the name goes into PHP's `$argv`. The binary reads no `phera.toml`.

## Syntax

The reference versions are vlucas/phpdotenv 5.8.0 and symfony/dotenv 8.1.6. Both accept blank
lines, full-line comments, ordinary assignments, an optional `export` prefix and `NAME=` for an
empty string. The table describes the cases where choosing a parser changes the result.

| Input | Phera | phpdotenv | Symfony |
| --- | --- | --- | --- |
| Variable names | phpdotenv names, including quoted names | Unicode lowercase/uppercase letters, marks, numbers, underscore and dot, including at the start | ASCII letter after optional leading underscores, then letters, digits and underscores |
| `NAME` on its own | Unset an earlier file value | Unset | Error |
| `NAME = value` | Allow spaces and tabs around `=` | Allow | Error |
| `A=two words  ` | Preserve inner whitespace and trim trailing spaces/tabs | Error | Error for ordinary unquoted words |
| `A=foo # note`, `A=# note` | Comment | Comment | Comment |
| `A=foo#bar` | Keep the embedded hash, as shell and Docker do | Value is `foo` | Value is `foo#bar` |
| `A='text'` | Literal bytes, including dollars and backslashes | Literal | Literal |
| Multiline quoted values | Allow either quote, normalize CRLF to LF | Double quotes can span lines. Single-quoted multiline fixture fails | Allow either quote |
| Double quotes with `\n`, `\r`, `\"`, `\\`, `\$` | Decode | Decode | Decode |
| Double quotes with `\t`, `\v`, `\f` | Decode as phpdotenv does | Decode | Keep backslash |
| Double quotes with an unknown escape such as `\q` | Preserve both bytes | Error | Preserve both bytes |
| `$NAME` outside single quotes | Expand from the inherited environment or preceding lines, empty if missing | Literal | Expand |
| `${NAME}` outside single quotes | Expand known names. Preserve the expression if missing | Same | Missing names expand to empty |
| `\$NAME` unquoted | A backslash protects the dollar. Each preceding pair contributes one backslash | Bare dollars stay literal, including the backslash | Honor backslash parity |
| `${NAME:-word}`, `${NAME:=word}` | Use the literal default for unset or empty names. `:=` also sets the file variable. Inherited names remain locked | No default operator | Supports both |
| `${${NAME}}` | Resolve an indirect name, up to 64 levels | Supports indirect names | No indirect names |
| Quotes, dollars or opening braces in a default | Syntax error | No default operator | Rejects these in an evaluated default |
| Text after a closing quote | Expect whitespace, a comment, or end of line | Rejects concatenation | Accepts adjacent value segments |
| `$(command)` | Error in unquoted and double-quoted values. A single quote or escaped dollar makes it literal | Literal text | Throws when command support is unavailable |
| UTF-8 byte order mark | Strip at the beginning of a file | Strip on file load | Error |
| Non-UTF-8 value bytes | Preserve | File reader converts to UTF-8 | Preserve |
| NUL in a value | Error. A child process environment cannot represent NUL | Can preserve an interior NUL | Parser uses NUL as a literal-dollar marker |

Expansion is a single pass over assignments. A value read from the environment is copied as bytes.
For example, with neither name inherited, `A=$B` followed by `B=$A` yields two empty strings.
The parser limits files and cumulative expansion per file to 32 MiB and individual values to
8 MiB. A nested default fails at its first unsupported character. Errors report the file,
one-based line and byte column, the validated name when available, and the expected syntax.
The profile portion of an automatically selected filename appears as `<APP_ENV>` in diagnostics.
The environment and parse state have no Rust `Debug` output.

## PHP request lifetime

The runtime snapshots the process environment once at startup. Each request reads the snapshot and
keeps its own changes in `RequestState`, as PHP-FPM restores the environment between requests.
On Unix names are case-sensitive. `getenv()` and `getenv(null)` enumerate the request's current
variables. Named lookups return a byte string, including an empty string, or `false` for absence.
The CLI's `local_only` argument consults the same local environment. PHP excludes names containing
a space, dot or opening bracket from enumeration and the environment superglobals. Named lookup and
the raw child environment retain them.

```php
<?php
putenv('MODE=test');  // getenv('MODE') returns 'test'
putenv('MODE=');      // getenv('MODE') returns ''
putenv('MODE');       // getenv('MODE') returns false
```

`$_ENV` and `$_SERVER` include the startup values, as in PHP CLI with `variables_order=EGPCS`.
CLI or HTTP server metadata takes precedence in `$_SERVER`. Writes to either PHP array and calls
to `putenv` are independent. Future process-spawning code must supply the pairs from
`RequestEnvironment::effective_environment()` to the child.

## Recorded comparisons

The offline Rust test compares 726 files against the results recorded by both libraries under
PHP 8.5.12. It compares the entire resulting map or rejection. The JSON Lines data also retains
each library's exception class and message, and represents input and result bytes in hex.
A phpdotenv unset is recorded as null and compared as an absent variable.
`cargo test -p phera-dotenv --test oracles -- --nocapture` prints the following breakdown.

| Snippet class | Both | phpdotenv only | Symfony only | Neither |
| --- | ---: | ---: | ---: | ---: |
| Bytes and BOM | 0 | 1 | 1 | 1 |
| Command substitution | 1 | 0 | 2 | 0 |
| Comments | 3 | 0 | 1 | 0 |
| Escaped unquoted dollar | 0 | 0 | 5 | 0 |
| Quoted escapes | 6 | 1 | 1 | 0 |
| Expansion | 1 | 1 | 10 | 0 |
| Names | 4 | 8 | 0 | 0 |
| Quotes | 3 | 1 | 1 | 0 |
| Unquoted inner spaces | 0 | 0 | 0 | 2 |
| Matrix, braced variable | 24 | 48 | 0 | 0 |
| Matrix, default operator | 8 | 16 | 16 | 32 |
| Matrix, bare variable | 8 | 16 | 16 | 32 |
| Matrix, empty value | 24 | 48 | 0 | 0 |
| Matrix, newline escape | 24 | 48 | 0 | 0 |
| Matrix, embedded hash | 16 | 32 | 8 | 16 |
| Matrix, plain string | 24 | 48 | 0 | 0 |
| Matrix, inner space | 16 | 32 | 0 | 24 |
| Matrix, unknown escape | 16 | 32 | 8 | 16 |
| Upstream fixture files | 13 | 6 | 2 | 3 |
| **Total** | **191** | **338** | **71** | **126** |

Each “neither” case has an independently asserted result in the Rust test.

- 26 inputs use the requested unquoted inner-space rule (two examples and 24 matrix cases).
- 96 inputs combine padded `=` with bare variable expansion (32), a default (32), a literal
  embedded hash (16), or a preserved unknown escape (16). Symfony rejects the padding.
  phpdotenv accepts the padding and interprets the value differently.
- Two inputs contain NUL, the byte example and upstream `nullbyte.env`. Phera rejects both.
- Upstream `commented.env` combines hash-without-space with unquoted inner spaces.
  Upstream `nested.env` combines phpdotenv's dotted and indirect names with bare-dollar expansion.

To refresh the recordings, install the pinned library source distributions in a scratch directory
outside the checkout. Their source checkouts include the upstream fixtures. PHP packages are used
only for regeneration. The fixtures retain their [upstream licenses](tests/fixture-licenses.txt).

```sh
scratch=$(mktemp -d)
composer --working-dir="$scratch" require --prefer-source vlucas/phpdotenv:5.8.0 symfony/dotenv:8.1.6
php crates/dotenv/tests/record.php "$scratch" > crates/dotenv/tests/oracles.jsonl
cargo test -p phera-dotenv
```

Under real PHP, both immutable loaders preserve a pre-existing variable when an application loads
its own file. Running those same applications under Phera currently stops in Composer's bootstrap
at `spl_autoload_register()`.
