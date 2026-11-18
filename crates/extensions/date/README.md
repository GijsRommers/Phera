# Date extension

Phera includes the PHP 8.5 date parsers, timezone database, formatters, native date
classes, exception hierarchy and their procedural entry points. The status below
covers the PHP surface and the remaining engine integration.

## Function and class inventory

| Function or class | Status | Coverage or remaining work |
| --- | --- | --- |
| `date_default_timezone_get` | DONE | Request-local default starts at UTC. |
| `date_default_timezone_set` | DONE | Identifier lookup and invalid-name notice. |
| `timezone_version_get` | DONE | Pinned PHP-compatible database version 2026.5. |
| `date` | DONE | Date format letters and byte escaping. |
| `gmdate` | DONE | UTC formatting with GMT abbreviation. |
| `idate` | DONE | Integer format letters and invalid-format diagnostics. |
| `getdate` | DONE | Local calendar fields and timestamp. |
| `localtime` | DONE | Numeric and associative output. |
| `checkdate` | DONE | Gregorian validation and PHP year bounds. |
| `mktime` | DONE | Local normalization, gaps, overlaps and integer overflow. |
| `gmmktime` | DONE | UTC normalization and integer overflow. |
| `date_parse_from_format` | DONE | Fields, modifiers, zones and diagnostic arrays. |
| `strtotime` | DONE | Timelib grammar, base filling and relative arithmetic. |
| `date_parse` | DONE | Absolute and relative fields with parser diagnostics. |
| `DateTimeInterface` | PARTIAL | Implemented by both classes with format constants and method declarations. The VM needs to restrict implementation by user classes and check native method types. |
| `DateTime` | PARTIAL | Constructors, factories, mutators, getters, arithmetic and direct magic methods work. Inherited static factories and native object hooks are pending. |
| `DateTimeImmutable` | PARTIAL | Mutators return new instances. Inherited static factories, ignored-return diagnostics and native object hooks are pending. |
| `DateTimeZone` | PARTIAL | Constructors, groups, offsets, transition ranges, metadata, abbreviations and direct magic methods work. Native clone, comparison and property projection are pending. |
| `DateInterval` | PARTIAL | ISO durations, relative strings, format codes, virtual arithmetic fields and direct magic methods work. Debug projection, clone and comparison need engine hooks. |
| `DatePeriod` | PARTIAL | Object constructors, ISO factory, getters, readonly properties with fresh snapshots, direct magic methods and the recurrence calculation work. Iteration, inherited ISO factories and native object hooks are pending. |
| `DateError` | DONE | Extends Error. |
| `DateObjectError` | DONE | Extends DateError and reports uninitialized objects. |
| `DateRangeError` | DONE | Extends DateError and reports timestamp and microsecond bounds. |
| `DateException` | DONE | Extends Exception. |
| `DateInvalidTimeZoneException` | DONE | Extends DateException and reports invalid timezone input. |
| `DateInvalidOperationException` | DONE | Extends DateException and reports unsupported subtraction. |
| `DateMalformedStringException` | DONE | Extends DateException and reports date string errors. |
| `DateMalformedIntervalStringException` | DONE | Extends DateException and reports ISO and relative interval errors. |
| `DateMalformedPeriodStringException` | DONE | Extends DateException and reports ISO period and recurrence errors. |
| `date_create` | PARTIAL | Mutable factory with false on parse failure. A named timezone with omitted datetime needs native defaults. |
| `date_create_immutable` | PARTIAL | Immutable factory with false on parse failure. A named timezone with omitted datetime needs native defaults. |
| `date_create_from_format` | DONE | Mutable format-directed factory. |
| `date_create_immutable_from_format` | DONE | Immutable format-directed factory. |
| `date_get_last_errors` | DONE | Request-local errors and warnings. |
| `date_format` | DONE | Shared formatter. |
| `date_modify` | DONE | Mutable modification with a warning and false on parse failure. |
| `date_add` | DONE | Shared interval addition. |
| `date_sub` | DONE | Shared interval subtraction with a warning for special intervals. |
| `date_diff` | DONE | Shared interval difference with DST and month-end corrections. |
| `date_timezone_get` | DONE | Shared timezone snapshot. |
| `date_timezone_set` | DONE | Mutable timezone conversion. |
| `date_offset_get` | DONE | Offset at the instant. |
| `date_time_set` | PARTIAL | Mutable time, including microseconds. Skipping second by name needs native defaults. |
| `date_date_set` | DONE | Mutable civil date. |
| `date_isodate_set` | DONE | Mutable ISO week date. |
| `date_timestamp_set` | DONE | Mutable Unix timestamp. |
| `date_timestamp_get` | DONE | Unix timestamp getter. |
| `timezone_open` | DONE | Shared constructor with a warning and false on invalid input. |
| `timezone_name_get` | DONE | Identifier, abbreviation or numeric offset name. |
| `timezone_name_from_abbr` | PARTIAL | Ordered reverse lookup. Skipping utcOffset by name needs native defaults. |
| `timezone_offset_get` | DONE | Timezone offset at a date's instant. |
| `timezone_transitions_get` | PARTIAL | Historical metadata and POSIX rules. Skipping timestampBegin by name needs native defaults. |
| `timezone_location_get` | DONE | Country, coordinates and comments. |
| `timezone_identifiers_list` | PARTIAL | Group and country filters. Skipping timezoneGroup by name needs native defaults. |
| `timezone_abbreviations_list` | DONE | Complete abbreviation table. |
| `date_interval_create_from_date_string` | DONE | Shared relative parser with a warning and false on failure. |
| `date_interval_format` | DONE | Shared interval format codes. |
| `date_sun_info` | DONE | Solar transit, rise, set and three twilight bands. |

Native argument coercion is shared with the VM. Gaps in optional named arguments
and arbitrary byte strings in error messages are tracked below.

### Deprecated PHP 8.5 surface

Omitted declarations include `strftime`, `gmstrftime`, `date_sunrise`,
`date_sunset`, `SUNFUNCS_RET_TIMESTAMP`, `SUNFUNCS_RET_STRING`,
`SUNFUNCS_RET_DOUBLE`, and the date classes' and interface's `__wakeup` methods.
The ISO string construction path is `DatePeriod::createFromISO8601String`.
The old string overload of the DatePeriod constructor emits a deprecation in PHP.
`DATE_RFC7231` and `DateTimeInterface::RFC7231` emit PHP’s deprecation when read.

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

The procedural wall-time matrix uses PHP's DateTime constructor as the oracle for
the Rust calendar resolver.
The parser suite adds **578,917 comparisons** (including five timezone-setting calls).
The generated table has **12,929 distinct strings**, each passed to `date_parse` and
`strtotime` with all eight fixed base timestamps and all five default zones. Additional
suites cover 19,932 ambiguous/corrupted byte strings, all 256 byte values, 10,000-byte
inputs, repeated parts, all embedded zone names and abbreviations, long fractions,
argument diagnostics, and integer overflow. No test requires the current clock.
See the class comparisons below for coverage of the native PHP objects.

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
parser/formatter round trips. DateTimeInterface exposes the same format strings.

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

CI uses PHP 8.5.12 with timezonedb 2026.5. When the system PHP uses a different
database, CI builds the pinned PECL release with `TIMELIB_SUPPORT_SLIM_FILE`
and loads it through the `-n` oracle wrapper. This matches the bundled PHP
database's explicit transition layout, including where it switches to POSIX
rules. The parser comparison results above are from the local PHP 8.5.12 run. Full local `make ci`
was not run because its prerequisite script provisions/connects to databases,
excluded by this task. `make fmt-check lint safe-rust`, the entire date crate's
oracle suite, runtime date tests, and CLI build passed locally.

## Class oracle comparisons

The same PHP scripts run on the release Phera binary and PHP 8.5.12. The scripts
use fixed instants and compare formatted dates, getters, diagnostic messages,
object identity, magic methods and aliases. The native matrices call registered
methods with argument coercion and compare their results separately.

| Matrix | Compared cases | Mismatches |
| --- | ---: | ---: |
| `modify` across sixteen zones and sixteen base instants | 197,120 | 0 |
| `add`, `sub` and `diff` around transitions, leap dates and month ends | 160,704 | 0 |
| `createFromFormat` format, modifier and invalid byte cases | 17,550 | 0 |
| DateInterval ISO durations and formatting | 4,734 | 0 |
| Internal DatePeriod sequences compared with PHP `foreach` | 2,560 | 0 |
| Solar information, including polar dates and argument errors | 3,376 | 0 |
| **Total native cases** | **386,044** | **0** |
| Release CLI scripts | 189 | 0 |
| Timezone catalog check within the CLI scripts | 2,557 output rows | 0 |

`tools/import_timezones.py` reads PHP 8.5.12's timelib data. The class catalog
retains the ordered identifier index, country flags, location coordinates,
explicit transition type indices, abbreviation data and POSIX rules. The
timezone catalog script checks every identifier against PHP with the default,
historical and future transition ranges. The date arithmetic uses the same
calendar and parser as the procedural functions and follows timelib's distinct
civil and elapsed-clock interval operations.

There are no new runtime dependencies. The existing `tz-rs`, `jiff-tzdb` and
`tzdb_data` pins are described above. The dev-only `phera-ext-core` dependency
loads the real Error and Exception parents in native test registries.

```sh
cargo build --release -p phera-cli
PHERA_BIN="$PWD/target/release/phera" cargo test --release -p phera-ext-date
```

## PHP 8.5.12 upstream date tests

The audit executed all 705 upstream FILE sections locally. The PHP 8.5.12
harness passed 693 and skipped 12. The Phera audit matched the upstream
EXPECT, EXPECTF or EXPECTREGEX output for 426 tests. Another 200 were blocked
by identified engine or runtime gaps, 67 exercised excluded deprecated
surfaces, and 12 were skipped by the reference harness. The Phera CLI has no
INI flag, so the audit translated valid `date.timezone` settings into explicit
calls and recorded tests that require INI or environment behavior. The
[per-file results](tests/classes-upstream-results.tsv) give the reason for
each difference.


## Engine integration

Fixtures in [tests/waiting](tests/waiting) are named for the required hook. To run
one as an ordinary comparison, set `PHERA_DATE_HOOKS` to its filename stem.
A comma-separated list selects several. The harness reports mismatches as failures.

| Waiting fixture | Current runtime behavior | Proposed API |
| --- | --- | --- |
| [`native-object-compare`](tests/waiting/native-object-compare.php) | Value operators compare classes and stored properties. Two date objects at the same instant can compare unequal. | `ClassEntry::compare: Option<fn(&mut dyn Context, &Object, &Object) -> Result<NativeComparison, NativeError>>`, with default, ordered and uncomparable results. |
| [`native-object-clone`](tests/waiting/native-object-clone.php) | The VM copies declared properties. Calling a method on a cloned date fails with an uninitialized-object error because its native state was lost. Zones, intervals and periods also lose their native state. | `ClassEntry::clone_native: Option<fn(&Object, &Object) -> Result<(), NativeError>>`, called with the original and new object before user clone hooks. |
| [`native-property-projection`](tests/waiting/native-property-projection.php) | Dump, print, export, JSON and array casts read stored properties. `json_encode($date)` returns `{}`. Interval metadata and period snapshots need operation-specific projections. | `ClassEntry::project_properties: Option<fn(&mut dyn Context, &Object, PropertyPurpose) -> Result<PhpArray, NativeError>>`, with the consumer identifying the PHP operation. |
| `native-object-serialization` | Direct `__set_state`, `__serialize` and `__unserialize` calls work. General `serialize` and `unserialize` entry points and their magic-method dispatch are absent. | The serializers should use `Context::call` for magic method dispatch and `Context::create_object` for constructor-free restoration. |
| [`native-object-iterator`](tests/waiting/native-object-iterator.php) | `foreach` uses ordinary property iteration and yields no dates. Native Traversable/IteratorAggregate dispatch and `DatePeriod::getIterator` are absent. | `ClassEntry::iterator: Option<fn(&mut dyn Context, Rc<Object>, bool) -> Result<Box<dyn NativeIterator>, NativeError>>` plus `Context::create_iterator` for the Iterator object. The period cursor and PHP comparison live in `src/classes/period_dates.rs`. |
| `native-called-class` | Native static handlers receive no called class. Inherited DateTime and DatePeriod factories return the declaring built-in class. | `Context::called_class(&self) -> Option<&str>`, scoped to the executing native call. |
| `native-named-defaults` | The VM fills skipped native named arguments with null. This can change a default or emit a null deprecation. | `Param::default: Option<ConstantValue>` and binding that distinguishes omitted arguments from explicit null. |

Additional native metadata gaps include DateTimeInterface's user implementation
restriction, signature variance for native methods and PHP's `#[NoDiscard]`
diagnostics on unused immutable mutator results. `NativeError::throw` takes a
Rust string, so parse diagnostics containing invalid UTF-8 are lossy.

The release scripts cover direct property reads, writes, `isset` and
`property_exists` on both classes. PHP exposes DateInterval's arithmetic fields
and `days` on initialized instances. `from_string` and `date_string` start
undefined. PHP's `var_dump` materializes the debug fields, which can make those
names readable after a dump. The ISO view includes the arithmetic fields and
`from_string = false`. The relative view contains `from_string = true` and the
original string. DatePeriod declares seven readonly properties. Its object-valued
reads produce fresh snapshots, and its dump view includes those snapshots and
recurrence flags. The projection fixture covers the remaining dump behavior.

Three changes outside the date crate support the public surface.
`ParamType::Class` and `ClassHandle::is_instance_of` reuse the VM's ancestry
lookup. `PropertyEntry::readonly()` uses its readonly checks.
`ClassEntry::property_access` dispatches direct instance access for native
properties. The call-handling directory has no date-extension edits.
