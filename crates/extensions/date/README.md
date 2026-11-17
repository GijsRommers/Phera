# Date extension: incomplete implementation

This branch implements the request timezone, calendar core, procedural formatters,
format-directed parsing, and the general date string parser. It does **not** complete PHP's date extension.
The PR must remain a draft until the missing functions, classes, and Linux gate pass.
Standard's existing clock and sleep functions remain in the standard extension.

## Function and class inventory

DONE means implemented and compared with PHP 8.5.12 in the local matrices below.
It does not assert exhaustive proof or a passing Linux CI run.

| Function or class | Status | Scope or missing work |
| --- | --- | --- |
| `date_default_timezone_get` | DONE | Request state starts at UTC. |
| `date_default_timezone_set` | DONE | Identifier lookup and PHP's invalid-name notice. |
| `timezone_version_get` | DONE | Reports the pinned PHP-compatible version, 2026.5. |
| `date` | DONE | All PHP date format letters and byte escaping. |
| `gmdate` | DONE | UTC formatting, including PHP's GMT abbreviation. |
| `idate` | DONE | Integer letters and invalid-format diagnostics. |
| `getdate` | DONE | Local calendar fields and timestamp. |
| `localtime` | DONE | Numeric and associative result arrays. |
| `checkdate` | DONE | Gregorian validation and PHP's year bounds. |
| `mktime` | DONE | Local normalization, gaps, overlaps, and integer overflow. |
| `gmmktime` | DONE | UTC normalization and integer overflow. |
| `date_parse_from_format` | DONE | Fields, modifiers, zones, warnings, and errors. |
| `strftime` | NOT DONE | C-locale formatting and deprecation. |
| `gmstrftime` | NOT DONE | C-locale UTC formatting and deprecation. |
| `strtotime` | DONE | Hand-written timelib grammar, base filling, relative arithmetic, and DST resolution. |
| `date_parse` | DONE | Absolute/relative fields, unset values, zones, and diagnostic arrays. |
| `DateTimeInterface` | NOT DONE | Interface and format constants. |
| `DateTime` | NOT DONE | Native state and all methods. |
| `DateTimeImmutable` | NOT DONE | Native state and all methods. |
| `DateTimeZone` | NOT DONE | Native class, constants, metadata, and methods. |
| `DateInterval` | NOT DONE | ISO duration parser, properties, arithmetic, and format. |
| `DatePeriod` | NOT DONE | Constructors and getters; iteration also needs engine work. |
| `DateError` | NOT DONE | Exception registration. |
| `DateObjectError` | NOT DONE | Exception registration. |
| `DateRangeError` | NOT DONE | Exception registration. |
| `DateException` | NOT DONE | Exception registration. |
| `DateInvalidTimeZoneException` | NOT DONE | Exception registration and messages. |
| `DateInvalidOperationException` | NOT DONE | Exception registration and messages. |
| `DateMalformedStringException` | NOT DONE | Exception registration and messages. |
| `DateMalformedIntervalStringException` | NOT DONE | Exception registration and messages. |
| `DateMalformedPeriodStringException` | NOT DONE | Exception registration and messages. |
| `date_create` | NOT DONE | DateTime wrapper. |
| `date_create_immutable` | NOT DONE | DateTimeImmutable wrapper. |
| `date_create_from_format` | NOT DONE | Construct native DateTime from parsed fields. |
| `date_create_immutable_from_format` | NOT DONE | Construct immutable native object. |
| `date_get_last_errors` | NOT DONE | Request-local constructor errors. |
| `date_format` | NOT DONE | DateTimeInterface wrapper. |
| `date_modify` | NOT DONE | DateTime wrapper. |
| `date_add` | NOT DONE | DateTime wrapper. |
| `date_sub` | NOT DONE | DateTime wrapper. |
| `date_timezone_get` | NOT DONE | DateTimeInterface wrapper. |
| `date_timezone_set` | NOT DONE | DateTime wrapper. |
| `date_offset_get` | NOT DONE | DateTimeInterface wrapper. |
| `date_diff` | NOT DONE | DateTimeInterface wrapper and exact DST/month-end algorithm. |
| `date_time_set` | NOT DONE | DateTime wrapper. |
| `date_date_set` | NOT DONE | DateTime wrapper. |
| `date_isodate_set` | NOT DONE | DateTime wrapper. |
| `date_timestamp_set` | NOT DONE | DateTime wrapper. |
| `date_timestamp_get` | NOT DONE | DateTimeInterface wrapper. |
| `timezone_open` | NOT DONE | DateTimeZone wrapper. |
| `timezone_name_get` | NOT DONE | DateTimeZone wrapper. |
| `timezone_name_from_abbr` | NOT DONE | Reverse abbreviation lookup. |
| `timezone_offset_get` | NOT DONE | DateTimeZone wrapper. |
| `timezone_transitions_get` | NOT DONE | DateTimeZone wrapper. |
| `timezone_location_get` | NOT DONE | DateTimeZone wrapper. |
| `timezone_identifiers_list` | NOT DONE | Identifier group and country filtering. |
| `timezone_abbreviations_list` | NOT DONE | Complete abbreviation metadata. |
| `date_interval_create_from_date_string` | NOT DONE | Relative interval parser. |
| `date_interval_format` | NOT DONE | DateInterval wrapper. |
| `date_sun_info` | NOT DONE | Deferred until all earlier work is complete. |
| `date_sunrise` | NOT DONE | Deferred until all earlier work is complete. |
| `date_sunset` | NOT DONE | Deferred until all earlier work is complete. |

Date class cloning, comparison, debug/JSON/serialization shapes, all factory methods,
microseconds, mutators, arithmetic, and constants are also NOT DONE. No placeholder
classes or functions have been registered for missing functionality.

## Calendar and timezone dependencies

The implementation uses pure Rust, with exact dependency pins:

- `tz-rs = 0.7.3`: TZif decoding and ordinary IANA offset lookup, with its `alloc`
  feature. Calendar arithmetic and timelib's full-width POSIX-rule behavior live here.
- `jiff-tzdb = 0.1.9`: embedded IANA **2026e** data for current and future rules.
- `tzdb_data = 0.2.5`: embedded IANA **2026b**, including the distinct pre-1970
  `backzone` histories that PHP retains. These histories are lost by some modern
  canonical aliases. Dublin/Eire use the newer data for their historical correction.

The real local PHP reports **8.5.12**, with `timezone_version_get()` **2026.5**.
[PHP's 2026.5 release](https://pecl.php.net/package/timezonedb/2026.5) uses IANA 2026e.
No runtime code reads the host's zoneinfo or invokes PHP. The reported version is
the compatibility target of the combined data, not the older backzone bundle alone.

The 2026b versus 2026e differences found while comparing with PHP include:

| Zones | Changed instant or interval (UTC) | PHP 2026.5 behavior |
| --- | --- | --- |
| Africa/Casablanca, Africa/El_Aaiun | 2026-09-20 01:00 onward | Permanent UTC+00. |
| America/Edmonton, America/Inuvik, Canada/Mountain | 2026-11-01 08:00 | UTC-06, CST. |
| America/Winnipeg, Canada/Central | 2026-11-01 07:00 | UTC-05, EST. |
| Europe/Dublin, Eire | 1925-09-20 02:00 to October 4 | Earlier return to GMT. |

Jiff distributes rearguard data; PHP exposes vanguard negative-DST flags.
The adapter preserves PHP's flags for Dublin/Eire, Morocco, and Windhoek.
The four legacy US POSIX identifiers also need their pre-1970 rules and abbreviations
restored. The oracle matrix checks these differences; this list is not a complete
release-to-release IANA database diff.

The abbreviation lookup in `data/abbreviations.tsv` contains the first mapping for
each abbreviation in PHP's timelib `timezonemap.h`. Its license is retained in
`data/TIMELIB-LICENSE.txt`. Format parsing follows `parse_date.re`'s format parser,
including its permissive numeric and timezone scanning. This is separate from the
general parser used by `strtotime` and `date_parse`. The fallback abbreviation table
is consulted by timelib only for offset/DST reverse lookup; these string parsers use
the first abbreviation mapping, as does PHP.

## Verified local comparisons

All oracle tests discover PHP with `testing::oracle::find_php()` and run it with `-n`.
They preserve mismatches and compare against real PHP, including native diagnostics.
Repeated debugging runs are not included in the count.

| Matrix | Compared calls | Mismatches |
| --- | ---: | ---: |
| Request timezone operations and version | 27 | 0 |
| Calendar/offset/abbreviation/DST core, 598 zones | 241,965 | 0 |
| Local wall-time resolution, eight zones | 9,600 | 0 |
| Procedural formatting, 38 zones | 732,868 | 0 |
| Procedural validation and integer boundaries | 2,602 | 0 |
| Format parser grammar, dates, zones, and bytes | 42,298 | 0 |
| Runtime compiler/VM calls, two requests | 334 | 0 |
| General parser generated strings and forty base/zone combinations | 530,094 | 0 |
| General parser examples | 1,305 | 0 |
| General parser corrupted/ambiguous inputs | 39,864 | 0 |
| General parser timezone names, fractions, and integer extremes | 3,052 | 0 |
| General parser hostile bytes and long inputs | 2,060 | 0 |
| General parser argument diagnostics | 30 | 0 |
| General parser ISO-week and ordinal boundaries | 2,512 | 0 |
| **Total** | **1,608,611** | **0** |

The core matrix includes transition boundaries, negative timestamps, leap dates,
years before 1900 and after 2100, and the endpoints of the signed 64-bit range.
Formatting crosses every format letter with fixed timestamps; parser matrices
include invalid UTF-8, NUL, incomplete fields, relative unit words, and malformed
numeric and timezone tokens. Nullable-now argument checks use literal formatting
so elapsed wall time cannot change their output.

The wall-time test calls PHP's DateTime constructor as the reference for the Rust
calendar resolver. It does **not** mean a Phera DateTime class has been implemented.
The parser suite adds **578,917 comparisons** (including five timezone-setting calls).
The generated table has **12,929 distinct strings**, each passed to `date_parse` and
`strtotime` with all eight fixed base timestamps and all five default zones. Additional
suites cover 19,932 ambiguous/corrupted byte strings, all 256 byte values, 10,000-byte
inputs, repeated parts, all embedded zone names and abbreviations, long fractions,
argument diagnostics, and integer overflow. No test requires the current clock.
The class matrix remains absent because classes are the next task.

Commands observed passing locally:

```sh
cargo test --release -p phera-ext-date -- --nocapture
cargo test -p phera-runtime --test date -- --nocapture
cargo clippy -p phera-ext-date --all-targets -- -D warnings
cargo build -p phera-cli
make fmt-check lint safe-rust
git diff --check
```

The combined run originally counted 26 request-zone cases; the added version case
was then run separately and passed, bringing that matrix to 27.

The optional `audit_phpt` example compares selected upstream PHPT **FILE sections**
by subprocess exit status, stdout, and stderr. Of 35 selected files, **22 matched**,
**0 differed**, and **13 have unsupported INI/environment setup**. It does not
implement PHPT EXPECT/EXPECTF or constitute a full PHPT harness. The general-parser audit below is separate. Evidence is retained under `target/date-phpt-audit` when run:

```sh
cargo run -p phera-ext-date --example audit_phpt -- PHP_SRC target/debug/phera
```

## General date string parser

`src/parse` implements byte scanning by hand, without a parser generator, a regex
engine, a C library, or calls to PHP in production. Each format family offers its
longest prefix; equal lengths use timelib's grammar order. Matching and field
assignment are separate, including partial results after duplicate specifications.
Weekdays, Monday-based weeks, month edges, and weekday counts retain their distinct
normalization stages. Calendar overflow rolls forward instead of clamping.

The scanner covers the documented numeric/textual dates and clocks, compound forms,
ISO weeks and ordinal dates, Unix timestamps with fractions, timezone offsets/names,
and relative words/units. Diagnostic positions are byte positions after timelib's
leading/trailing whitespace trimming. C's whitespace and non-finite float-to-integer
behavior differ on macOS/ARM and Linux/x86; the adapter follows the target platform.
The thirteen non-deprecated procedural `DATE_*` format constants are registered for
parser/formatter round trips. No DateTime classes were added.

The optional `parser_audit` example discovers upstream tests mentioning either parser.
It copies FILE sections, translates `date.timezone` INI settings, and uses PHP's
tokenizer to pin procedural clock calls to **1706704496**. Both runtimes execute the
same copy. It honors upstream SKIPIF and compares exit status, stdout, stderr, and
PHP's result against EXPECT/EXPECTF. Source tests are never modified.
`phera-phpt` is a **dev-only** dependency for section parsing and EXPECTF matching;
there are no new runtime dependencies.

```sh
cargo run -p phera-ext-date --example parser_audit -- PHP_SRC target/debug/phera
cargo run --release -p phera-ext-date --example benchmark_parser
```

The audit deliberately exits unsuccessfully for missing unrelated runtime APIs and
retains per-test outputs and reasons in `target/date-parser-phpt/report.txt`.
Current audit: **84 pass both PHP comparison and upstream EXPECT/EXPECTF**, **2 fail**,
**13 need DateTime classes**, **10 need other unsupported APIs**, and **2 skip on
64-bit platforms**. The broader discovery includes two class tests that mention
`strtotime` in comments.

- `bug46111.phpt`: missing `timezone_identifiers_list()` (outside this parser task).
- `format-negative-timestamp.phpt`: missing standard `getrandmax()`.
- Other dependencies: five environment/INI tests, four sun-function tests, and one
  IntlDateFormatter test. All results are retained; no failure is marked expected.
  [Per-file audit snapshot](tests/parser-upstream-results.txt).

The existing date core needs no additional engine API for either parser function.

For **one million calls across twenty common formats**, three alternating subprocess
runs with `/usr/bin/time -p` measured these CPU times (user plus system):

| Run | PHP 8.5.12 | Rust release native entry point |
| --- | ---: | ---: |
| 1 | 0.76 s | 0.82 s |
| 2 | 0.76 s | 0.89 s |
| 3 | 0.81 s | 0.95 s |
| Median | **0.76 s** | **0.89 s** |

The median ratio is **1.17x**, within the requested 2x limit. The benchmark uses UTC,
a fixed base, and identical checksums (1707946012000000). It measures native function
calls, not compiler/VM throughput. Process startup is included; compilation is not.
Wall-clock time is not used because other workloads share this machine.

The existing CI installs `timezonedb-2026.5`, loads it through the `-n` oracle wrapper,
and verifies its version. Linux checks are still running for the parser push;
local success is not evidence of a completed hosted gate. Full local `make ci`
was not run because its prerequisite script provisions/connects to databases,
excluded by this task. `make fmt-check lint safe-rust`, the entire date crate's
oracle suite, runtime date tests, and CLI build passed locally.

## Engine integration still needed

These are specific future integration gaps for the classes, not blockers for the
completed procedural parser. This PR does not modify these engine paths.

- `crates/value/src/ops/compare.rs` compares class names and property arrays.
  There is no native-state comparison callback, including cross-class comparisons
  between DateTime and DateTimeImmutable.
- `crates/vm/src/classes/instantiation.rs::clone_object` copies ordinary property
  arrays, not `Object::native`, and does not invoke `__clone`. Only Closure has a
  special native-state copy path. A native clone callback or equivalent is needed.
- `crates/vm/src/iteration.rs` iterates visible object properties. It does not
  dispatch IteratorAggregate/Traversable, which DatePeriod iteration needs.
- `crates/extensions/core/src/dump/mod.rs` reads the property array directly;
  there is no native debug-property or `__debugInfo` dispatch.
- `crates/extensions/json/src/encode.rs` reads public properties directly.
  Native DateTime JSON fields would require property projection or a hook.
  JsonSerializable dispatch is itself documented as awaiting VM integration.
- No `serialize`/`unserialize` functions are registered in standard/core here;
  date serialization methods alone would not supply end-to-end serialization.

The API already provides request state, native class methods/constants/properties,
`Object::native`, object construction, and typed exception throwing. Those pieces
are sufficient to continue implementing the date classes after the parser.

The parser required no engine or extension-API changes. Merging `origin/main` into
this branch required no date-crate compatibility fixes. The two general-parser
functions have no known local oracle mismatches; hosted CI remains a separate gate.
