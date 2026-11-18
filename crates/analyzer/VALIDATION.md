# Level 0 and 1 validation

Measurements used an Apple M4 running macOS 27.2, release Phera, PHP 8.5.12 and
PHPStan 2.3.6. PHPStan was installed with Composer only in the requested external
scratch project. No PHP dependency or downloaded project is part of this repository.

## Corpus

There are **468 small PHP files** with rule headers and independent `.expect`
line sets for both levels. **240** have one deliberate error line; **228** are
clean, suspicious-looking programs or suppression/adoption examples. Every file
with a reported error has exactly one expected error line. CLI tests run each
file independently at both levels.

PHPStan agrees on both levels for **384 files**. **84 files** have explained
differences, with **zero unexplained differences**. Every differing file/level
and both line sets are recorded in [DIFFERENCES.md](tests/corpus/DIFFERENCES.md).
The comparison validates those exact tuples, rather than accepting a filename
with any explanation. PHPStan's complete JSON is retained in scratch output.

Differences include requested checks enabled earlier than PHPStan, PHPStan style
checks outside these two levels, compatible suppression syntax and specific
PHP-semantic disagreements. Targeted PHP 8.5 programs settled reference binding,
finally transfers, closure rebinding, argument names and return behavior. None of
the random or potentially infinite corpus programs was executed.

The normal comparison regenerates PHPStan observations. `--reuse-phpstan` is an
optional acceleration requiring identical source SHA-256 and PHPStan version;
Phera is always rerun. Semantic `.expect` files are maintained separately.

```sh
crates/analyzer/tests/compare-with-phpstan.sh --phpstan /scratch/phpstan/vendor/bin/phpstan
```

## Complete real-project production sets

The following repositories were downloaded into scratch. The file lists exclude
their test suites and include the production dependency paths explicitly. No
source file was removed to obtain a clean report.

| Project | Pinned revision | Input roots | Files |
| --- | --- | --- | ---: |
| Symfony OptionsResolver | `0a2aa769aced59924bbf5a370aa3def83c836a0f` | root and `vendor/symfony` | 14 |
| League Container | `4fee6c75b3368dba3682a67439b57f8e24ab3eb7` | `src`, `vendor/psr` | 64 |
| ircmaxell/password_compat | `dfdbc467fba0d1db4c2f51fda58e1717046f3284` | `lib` | 1 |

These cover a framework component, a namespaced library using traits/interfaces,
and an older procedural polyfill. Native PHP 8.5 function-existence guards make
the obsolete password fallback inactive; the checker respects that guard.

Each number below is the median of three alternating runs. PHPStan gets a fresh
result-cache directory each run. Both tools receive the same exact PHP file
list at level 0. Wall time includes process startup and excludes compilation.

| Project | Phera seconds | Phera files/s | PHPStan seconds | PHPStan files/s |
| --- | ---: | ---: | ---: | ---: |
| OptionsResolver | 0.0112 | 1,245 | 1.0303 | 13.59 |
| Container | 0.0138 | 4,624 | 1.2999 | 49.24 |
| password_compat | 0.0095 | 106 | 0.9157 | 1.09 |

Phera reported **zero diagnostics and zero observed level-0 false positives** on
all three sets, with no crashes. PHPStan reported no source errors on the first
and third sets. Container produced four unmatched-ignore reports for identifiers
used at higher PHPStan levels; unused-ignore checking is outside Phera's scope.
Small-project timings include startup overhead and differing rule workloads;
they do not establish equivalent analysis completeness or large-project speed.

[real-code.json](tests/validation/real-code.json) records the exact file lists,
dependency versions/revisions, all three samples, raw diagnostics, analyzer code
commit and binary SHA-256. Later commits add corpus/documentation evidence without
changing that analyzer code. Reproduce with externally downloaded projects and
production Composer dependencies in a scratch directory:

```sh
cargo build --release -p phera-cli
python3 crates/analyzer/tests/benchmark-real-code.py \
  --code-root /scratch/real --phpstan /scratch/phpstan/vendor/bin/phpstan \
  --output /scratch/timings
```

## Phera's own PHP fixtures

All **1,333 PHPT `--FILE--` sections** and the **one standalone PHP fixture**
were checked independently. This preserves intentional duplicate declaration
names across tests. Neither tool executes their PHP. Runtime-created include
files are consequently not materialized or treated as input declarations.

Both tools use one process per fixture in separate eight-worker phases. Phera's
final release binary took **2.338 seconds (571 files/s)**; PHPStan level 0 took
**202.329 seconds (6.59 files/s)**. These per-file launch measurements are distinct
from the single-project measurements above. The PHPStan phase was retained when
refreshing Phera after the last analyzer fix; every extracted input was verified
byte-for-byte and the PHPStan version checked before reuse.

Phera reported **67 diagnostics**. Each was checked against PHPStan at the exact
line and identifier: **52** match level 0 and the remaining **15** match level 1.
There are **zero unconfirmed reports and zero observed level-0 false positives**.
The latter group comprises the requested earlier checks for definite global reads,
constants and excessive arguments. Fixture expectations also establish the intended
runtime warnings/errors. Successful rebinding examples are silent even where
PHPStan reports `$this` prematurely.

[fixtures-review.tsv](tests/validation/fixtures-review.tsv) lists every diagnostic
and its corroborating PHPStan level. [fixtures-summary.json](tests/validation/fixtures-summary.json)
retains timings, input SHA-256 values and the checked binary SHA-256. The complete
raw per-file observations remain in scratch. Reproduce with:

```sh
python3 crates/analyzer/tests/check-project-fixtures.py \
  --phpstan /scratch/phpstan/vendor/bin/phpstan --output /scratch/fixtures
```

## Fixes established during validation

- Correct lexical namespace fallback, ASCII identifier folding, inheritance,
  trait aliases/contracts, enum members and relative `self`/`static`/`parent` types.
- Preserve only continuing paths through expressions, loops, switch, exceptions
  and finally; return/throw/break/continue carry separate scopes.
- Model reference rebinding, escaping array cells, global writes and unknown
  scope effects without retaining stale class or callable facts.
- Separate a closure's lexical class from its bound object. Later rebinding is
  valid; a provably unbound invocation reports the original `$this` line.
- Infer callable-object and explicit closure invocation signatures, while keeping
  unknown engine-generated Closure parameter lists unknown.
- Generate from pinned PHP 8.5 Git objects rather than the local php-src master,
  which currently contains a different PHP version. The default PHP 8.5 inventory
  test checks every loaded function, class, constant and method parameter name.

## Gate and scope

Local validation includes workspace formatting, Clippy with warnings denied,
the unsafe-token guard, analyzer/CLI tests and workspace tests against real
PHP 8.5 excluding the PDO crate. No local Docker service or database was touched.
The full database prerequisite/test gate runs in the existing hosted CI workflow;
the PR's current check results are the authority for that gate.

No parser, VM or value-crate edits were needed. The committed table contains
10,072 declarations from 121 parsed stub files, with zero skipped declarations.
Generation provenance and exact reserved-name parser workarounds are recorded in
[generation.txt](data/generation.txt).

Only the requested two levels are implemented. The scope, flow, index, callable
signature and scalar/union/class/mixed lattice support later argument/return type
rules. PHPDoc generics, shapes, full intersection reasoning and additional levels
are not implemented. Uncertain dynamic behavior and analysis beyond the explicit
depth limit stay silent; this is not a claim to prove every PHP program correct.
