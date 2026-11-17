# Phera check

`phera check` reads PHP without executing it. It shares Phera's source, lexer,
parser and AST crates and has no runtime, VM, PHP or third-party dependency.

```sh
phera check src tests
phera check --level 0 --format json src
phera check --format github --no-progress src
phera check --generate-baseline check-baseline.json src
```

Exit codes: **0** means no unsuppressed diagnostics, **1** means diagnostics were
reported (including syntax errors), **2** means invalid options, configuration or
unreadable inputs. The default level is **1**. Positional paths override configured
paths; `--level` overrides the configured level. `--` ends options. Progress is
written only to an interactive stderr; `--no-progress` disables it.

An optional `phera.toml` in the working directory supports:

```toml
[check]
level = 1
paths = ["src", "tests"]
exclude = ["tests/generated/*", "src/cache"]
baseline = "check-baseline.json"
```

Arrays may span lines. Basic and literal strings and comments are supported.
Other tables are ignored. Paths and baseline names are relative to the working
directory. Exclusions match a path, its subtree, or a `*`/`?` wildcard pattern;
`*` also matches directory separators. Files are sorted and deduplicated. Directory
symlinks are not followed. `vendor` directories are skipped unless an input path
explicitly names `vendor` or something below it. To include dependency declarations,
pass their paths too, for example `phera check src vendor/psr`.

## Levels and identifiers

Every rule is a small type implementing `rules::Rule`, in its own file. Rules
consume facts established by the analyzer. There is no runtime evaluation.

| Level | Identifier | Behavior |
| --- | --- | --- |
| 0 | `phpstan.parse` | Report a file's first syntax error and continue other files |
| 0 | `class.notFound` | Unknown classes in construction, instanceof, catch, static references, types and inheritance |
| 0 | `function.notFound` | Unknown functions, with PHP namespace fallback |
| 0 | `method.notFound` | Undefined methods on `$this`, class names, self/static/parent |
| 0 | `property.notFound`, `staticProperty.notFound` | Undefined properties on those receivers |
| 0 | `constant.notFound`, `classConstant.notFound` | Unknown global and class constants |
| 0 | `arguments.count` | Too few/many known arguments, except uncertain spreads and variadics |
| 0 | `argument.unknown` | Unknown names in direct function/method arguments |
| 0 | `variable.undefined` | Certainly undefined variables and `$this` outside object contexts |
| 0 | `new.abstract`, `new.interface` | Instantiation of abstract classes or interfaces |
| 0 | `method.staticCall` | Static calls to non-static methods |
| 0 | `method.abstract` | Missing inherited abstract/interface implementations |
| 1 | `variable.undefined` | Possibly undefined variables after control-flow joins |
| 1 | `constant.notFound` | Unknown magic constant spellings |
| 1 | `argument.unknown` | Unknown names on inferred closure/arrow/first-class callables |
| 1 | `return.void`, `return.missing` | Values returned from void functions, and missing declared returns |

Member signatures can be inferred from parameter/property/return types, `new`,
and first-class callables. Undefined-member rules on arbitrary inferred expressions
are reserved for later levels; known signatures still enable argument checks.
Magic member handlers, traits, promoted properties, enum members, inherited methods,
nullable types and unions are modeled without asserting unknown facts.

## Stable JSON, version 1

JSON stdout is one object, followed by a newline:

```json
{"version":1,"diagnostics":[{"file":"src/a.php","line":3,"identifier":"class.notFound","message":"Class Missing not found."}]}
```

Keys and diagnostic ordering are stable: file, line, identifier, message. Lines
are one-based. Paths use `/`; relative inputs produce relative paths. There are
no timings or progress messages in JSON stdout. Consumers should ignore unknown
keys in future versions. Text output groups reports by file; GitHub output escapes
workflow annotation data and properties.

A generated baseline uses the same objects, one per line, within the version 1
JSON envelope. Its reader accepts the generator's canonical format. Matching is
exact over **file, line, identifier and message**; moving or changing a diagnostic
makes it visible again. Generating a baseline writes the current unsuppressed
report and still exits 1 if it contains diagnostics. It does not apply an existing
baseline first. Unused baseline entries cause no error.

## Suppression

The checker recognizes actual comments, not strings that resemble comments:

```php
missing(); // @phpstan-ignore function.notFound
// @phpstan-ignore-next-line
missing();
missing(); // @phpstan-ignore-line
// @phera-ignore
missing();
/** @psalm-suppress UndefinedFunction */
function example() { missing(); }
```

`@phera-ignore` accepts an optional comma-separated identifier list; without one
it suppresses the same or next code line. PHPStan identifier ignores target the
same line when following code, otherwise the next code line. Psalm suppressions
apply to the following statement/declaration, including its body. Overlapping
Psalm names are mapped to Phera/PHPStan identifiers; unsupported Psalm rule names
do nothing. Phera does not report unused suppressions.

## Builtin data

The committed table is generated from **PHP 8.5.12**, php-src commit
`136e049ac10b032c83018e65b63b6df858943750`. Generation reads Git objects in the supplied
repository and never checks out another branch or edits php-src:

```sh
cargo run -p phera-analyzer --bin generate-stubs -- /path/to/php-src
# An optional second argument selects another PHP 8.5 Git reference.
```

This chooses committed generation rather than build-time generation: ordinary
Phera builds must work without php-src, PHP, or a network connection. All production
stubs under Zend, main and ext are included, covering default and optional bundled
extensions on all platforms. Development-only zend_test and dl_test are excluded.
CLI stubs are included as well. Conditional alternatives use the last declaration,
which selects the default OpenLDAP signature rather than Oracle LDAP's alternative.
No PECL extension stubs or PHPDoc generics are invented.

`data/generation.txt` records counts, provenance and every parser workaround.
`clone`, `exit` and `die` are stub pseudo-functions whose reserved-word names are
renamed for parsing and then restored in the index. No parser changes are needed.
Attributes parse directly. Native C registrations absent from stubs add stream
constants, the CLI process-title constant, the SVG image constant, MHASH constants,
the documented DOM exception alias and the engine's Closure::__invoke placeholder.
Its actual callable parameter list comes from the closure expression when known.
The generator fails rather than committing a table with skipped stub files.

`php85.tsv` is internal, deterministic generated data: C records classes; F/M
functions/methods; P properties; G/K global/class constants. Member records follow
their class. Names, type encodings, modifier flags and source locations are retained.
`@Name` is a class type; `|` separates union members; source locations are `file:line`.
Function parameter fields carry name, type, optional/variadic/reference flags.
The public JSON diagnostic schema is independent of this internal format.

## Architecture and bounds

Discovery and parsing use standard-library threads on all available cores, with
a parser-sized stack on each worker. An index is built before bodies are checked.
Namespace imports preserve PHP's class/function case rules and constant case
sensitivity. Function and constant fallback is used only for unqualified names.
Ancestry and trait linking use work lists, with cycle detection and a fixed-point
bound. Unknown ancestors prevent unsupported missing-member conclusions.

Scopes track definite, possible and unknown definitions, reference aliases,
constant facts and existence guards. Joins keep only facts shared by continuing
paths. Loops, switch fallthrough, exception edges and finally transfers have their
own modules. Dynamic writes, extract/include/eval and unknown reference parameters
remain uncertain. Closures have explicit capture scopes; arrows capture automatically.

The initial lattice has scalars, null, arrays, object, classes, unions, mixed, void,
never and callable signatures. PHPDoc generics, shapes, array element types, full
intersection reasoning, argument type checking, dead-code reports and plugins
are intentionally left for later levels. The current scope/flow/type/rule boundary
provides their foundation.

Expression/body nesting is bounded at 256 on an explicitly sized stack. Guard,
index and structural walks are iterative, including long binary/postfix chains.
Beyond the analysis bound, facts become unknown and missing-return reports are
suppressed. The parser independently bounds nesting at 1000. No unsafe code,
allow/path attributes, leaked boxes or custom macros are used.

## Verification

```sh
cargo test -p phera-analyzer
cargo test -p phera-cli --test check_corpus --test check_options
PHPSTAN=/outside/repo/vendor/bin/phpstan crates/analyzer/tests/compare-with-phpstan.sh
```

The comparison script regenerates `.phpstan` line sets beside each fixture, saves
full diagnostic JSON under `/private/tmp/phera-check-comparison` by default, prints
every differing file/level, and fails on any difference without an exact filename
entry in `corpus/DIFFERENCES.md`. `.expect` files are separately maintained semantic
expectations for Phera. The public CLI is tested for every corpus file at both levels.
PHPStan/Composer dependencies belong only in a scratch project outside this repository.
