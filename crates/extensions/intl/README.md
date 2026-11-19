# PHP intl

Phera's intl module uses ICU4X 2.4 compiled data (Unicode 18, CLDR 49).
PHP's intl module uses the ICU4C installed on the oracle machine. This Mac's
PHP 8.5.12 uses ICU4C 79.1 (Unicode 18, CLDR 49). The
`INTL_ICU_VERSION` and `INTL_ICU_DATA_VERSION` values describe Phera's
backend and CLDR release respectively.

The oracle checks PHP values, changed references and diagnostics. Run
`cargo test -p phera-ext-intl --test php_oracle -- --nocapture` with PHP
8.5 and intl available. Hosted tests load intl through the shared PHP wrapper
when it is a shared module.

| API | Coverage |
| --- | --- |
| `Normalizer`, `normalizer_*` | Five forms including NFKC casefold, normalization checks and raw decompositions |
| `grapheme_*` | Length, substring, splitting, extraction, searching and weighted edit distance. Search uses grapheme sort keys and needs additional coverage for contractions spanning clusters. PHP 8.5.12 has no `grapheme_similar`. |
| `Locale`, `locale_*` | Default locale, locale IDs, lookup, HTTP negotiation, likely subtags and localized names. Legacy ICU identifiers and display keywords have gaps. |
| `NumberFormatter`, `numfmt_*` | Decimal, scientific, percent, currency and compact notation, with parsing and mutable settings. Custom rule-based formats, spellout, ordinal and duration raise unsupported errors. Pattern syntax, attributes and parse edge cases are partial. |
| `IntlDateFormatter`, `datefmt_*` | Gregorian styles and patterns, PHP date objects, timestamps, local-time arrays and parsing. Traditional calendars, relative styles, calendar objects and unusual parse fields are partial. |
| `idn_to_ascii`, `idn_to_utf8` | UTS #46 mapping, Punycode, transitional forms and error details. |
| `intl_*error*`, `intl_is_failure`, `IntlException` | Request-local last error, ICU error names and numeric compatibility constants |
| `Collator`, `collator_*` | Locale comparisons, array sorting and keys, strength, case, punctuation and numeric options. Sort-key bytes and some ICU-specific attributes differ. |
| `MessageFormatter`, `msgfmt_*` | MessageFormat 1 with nested select, cardinal and ordinal plural branches, and number, date and time arguments. Indexed parsing reuses localized number and date parsers, including fractional seconds. Selection parsing, parse timezone defaults and diagnostics are partial. |
| `Transliterator`, `transliterator_*` | Compound named transforms, Latin-ASCII, normalization, case mapping, and selected script and BGN romanizations. User rules, inverse IDs and full ICU transform coverage are partial. |
| `IntlTimeZone`, `intltz_*` | Zone construction, offsets, aliases, Windows mappings and conversion to PHP date zones. ICU-specific enumeration, rule comparisons and localized zone names are partial. |
| `IntlCalendar`, `IntlGregorianCalendar`, `intlcal_*`, `intlgregcal_*` | Millisecond instants, field access and mutation, conversion to PHP dates, Gregorian cutover and locale week data. Field limits, non-Gregorian mutation and ICU wall time settings are partial. |
| `ResourceBundle`, `resourcebundle_*` | Selected compiled locale tables and native-endian standalone ResB directories with read-only indexing and iteration. Shared pools, aliases, complete locale inheritance and many built-in keys remain incomplete. |
| `IntlBreakIterator`, `IntlIterator`, `IntlPartsIterator` | Character, code point, word, line and sentence iteration, with PHP byte offsets and parts. Title and custom ICU break rules are unsupported. |
| `IntlListFormatter` | PHP 8.5 conjunction, disjunction and unit patterns in wide, short and narrow widths. |
| `IntlChar` | Code points, case mapping, combining classes, digit conversion and common Unicode categories and predicates. The full ICU property and name APIs are unsupported. |
| `Spoofchecker` | Reports unsupported. ICU4X does not supply the corresponding ICU spoof checker. |

The two normalization mapping files under `data/` are derived from Unicode
18.0.0 [UnicodeData.txt](https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt)
(decomposition field, compatibility rows) and
[DerivedNormalizationProps.txt](https://www.unicode.org/Public/18.0.0/ucd/DerivedNormalizationProps.txt)
(`NFKC_CF` property). The Unicode data license is included alongside them.
The ICU numeric constants and error names are the public PHP intl and ICU
error-code tables as exposed by PHP 8.5.12 with ICU 79.1.

CLDR 49's number patterns, locale display patterns, Gregorian date styles,
currency-region map and reverse Windows zone map are compiled into the
extension. Their source is the
[CLDR JSON release](https://github.com/unicode-org/cldr-json/tree/49.0.0).
The [IANA Language Subtag Registry](https://www.iana.org/assignments/language-subtag-registry/)
supplies grandfathered tags.

Standalone resource directories use the native-endian ResB format. The
loader supports strings, binary entries, integers, integer vectors, arrays and
tables. It resolves directories against PHP's request working directory and
reads files asynchronously. The implementation follows the
[ICU resource data layout](https://github.com/unicode-org/icu/blob/main/icu4c/source/common/uresdata.h).

## Locale data differences

| Call | PHP / ICU4C CLDR | Phera / ICU4X CLDR | Difference and disposition |
| --- | --- | --- | --- |
| Decimal, currency and percent grouping of four-digit numbers in `es_ES` and `it_IT` | 49 | 49 | Fixed. PHP's legacy formatter groups before ICU4X's default minimum. |
| Half-integer `JPY` values in `ja_JP` | 49 | 49 | Fixed. Round half-even before the ICU4X currency formatter applies its precision. |
| Long date-time style in `ar_EG`, Brussels GMT offsets at Unix timestamps 0 and 1724727600 | 49 | 49 | Fixed. Match ICU4C's unmarked sign in the rendered Arabic time-zone field. |
| Generic `Any-Latin/BGN` transform | 49 | 49 | Fixed. ICU4C registers language-specific BGN identifiers and rejects this generic ID. Symfony falls back to `Any-Latin`. |
| Explicit date patterns using `ss.SSS` in `fr_FR`, `nl_BE` and `ar_EG` | 49 | 49 | Fixed. PHP keeps the literal dot between seconds and milliseconds. |
| `INTL_ICU_VERSION` and `INTL_ICU_DATA_VERSION` | 49 (`79.1` for both PHP constants) | 49 (`2.4.0` and `49`) | Accepted. Phera reports the ICU4X backend and compiled CLDR release. The PHP eligibility sweep covers ICU-version-gated fixtures on this host. |
| `IntlTimeZone::getIDForWindowsID` with India or an absent Pacific region override | 49 | 49 | Fixed. Resolve through the world region and return ICU's legacy `Asia/Calcutta` alias. |
| `DECIMAL_COMPACT_SHORT` for legacy `en_UK` | 49 | 49 | Open. PHP renders `1.2M` and Phera resolves the legacy region alias and renders `1.2m`. |
| Traditional Hebrew and Hijri formatting in `en_US` and `pt_PT` | 49 | 49 | Open. Traditional calendars currently use medium fields. The requested weekday, time-zone and custom-pattern fields can differ. |

## Validation on macOS

PHP 8.5.12 with ICU4C 79.1 matched all 8,864 focused comparisons. The 8,647
Stage 1 calls include 675 display names, 750 number and date formats over
the 15 core locales, and a Gregorian calendar object. The 217 Stage 2 calls
cover collation, transliteration,
MessageFormat, time-zone identifiers and character operations.
The three comparisons for negative offsets below `INT32_MIN` are tagged with
PHP 8.5.13. PHP 8.5.12 truncated those offsets before parsing.

The scratch Composer application matched 126 outputs byte for byte.
It exercised five Twig filters and three Symfony validators over
`en_US`, `en_GB`, `nl_BE`, `nl_NL`, `fr_BE`, `fr_FR`, `de_DE`, `es_ES`,
`it_IT`, `pt_BR`, `ja_JP`, `ar_EG`, `ru_RU`, `zh_CN` and `hi_IN`. The six
AsciiSlugger inputs used Latin, Greek, Cyrillic, Arabic, Devanagari and Han.
A 195-output supplemental probe matched list types and widths, sample
collation bytes and sorting, calendar fields, break boundaries and time zones.
The Composer application used Symfony Intl and String 8.1.10, Symfony
Validator 8.1.11, Twig and Twig Intl Extra 3.30.0.

## PHP source tests

The native PHP 8.5.12 suite reported 455 passes, 3 failures, 47 skips and
1 upstream expected failure. The native failures were the two older
`dateformat_formatObject_*_icu72-1` fixtures, which put the Portuguese GMT
label before the offset, and `timezone_IDforWindowsID_basic_icu76_1`, which
expected an older Canadian Windows-zone mapping. PHP 8.5.13 adds ICU 79.1
variants for those fixtures.

Phera's common runner on the `cb147514` build reported 209 passes,
226 failures, 70 skips and 1 upstream expected failure against the 8.5.12
fixtures. The sweeps below use PHP 8.5.12 to evaluate the original eligibility
probes and run the unchanged fixture bodies on Phera. This exercises ICU-version-gated
cases even though Phera reports an ICU4X version. The common runner excludes
unsupported harness sections, including `EXPECTREGEX`, some regex
placeholders, and the `memory_limit` setting. The 8.5.13 eligibility sweep
also skips 39 fixtures through their own ICU version conditions. The offset
regression added in PHP 8.5.13 passes on Phera.

The cells record pass / fail / skip / upstream expected failure.

| Directory in `ext/intl/tests` | PHP 8.5.12, common runner | Phera, 8.5.12 fixtures | Phera, 8.5.13 fixtures |
| --- | ---: | ---: | ---: |
| Root | 426 / 3 / 64 / 1 | 212 / 217 / 64 / 1 | 215 / 215 / 67 / 1 |
| `formatter` | 2 / 0 / 0 / 0 | 1 / 1 / 0 / 0 | 1 / 1 / 0 / 0 |
| `listformatter` | 8 / 0 / 0 / 0 | 8 / 0 / 0 / 0 | 8 / 0 / 0 / 0 |
| `locale` | 2 / 0 / 0 / 0 | 1 / 1 / 0 / 0 | 1 / 1 / 0 / 0 |
| **Total** | **438 / 3 / 64 / 1** | **222 / 219 / 64 / 1** | **225 / 217 / 67 / 1** |

| Component in PHP 8.5.13 | Phera | Principal remaining failures in this sweep |
| --- | ---: | --- |
| Break iteration | 15 / 11 / 7 / 0 | Rule-based constructors, binary rules, some cursor and part diagnostics |
| Calendars | 32 / 43 / 0 / 0 | Field bounds, defaults, non-Gregorian arithmetic, week rules and wall-time settings |
| Collation | 11 / 5 / 2 / 0 | Constructor diagnostics, raw sort-key fixtures and mutations during PHP comparison |
| Common and regressions | 56 / 35 / 13 / 0 | Error state, reflection signatures, invalid input diagnostics and mixed-API regressions |
| Converters | 0 / 17 / 1 / 0 | UConverter is outside the requested scope |
| Date formatter | 18 / 14 / 12 / 1 | Traditional calendars, relative styles, invalid patterns and reference diagnostics |
| Date pattern generator | 0 / 3 / 1 / 0 | IntlDatePatternGenerator is unsupported |
| Grapheme functions | 10 / 6 / 1 / 0 | Locale-dependent matches across clusters and some by-reference diagnostics |
| IDNA | 4 / 0 / 0 / 0 | All exercised fixtures passed |
| List formatter | 8 / 0 / 0 / 0 | All exercised fixtures passed |
| Locale | 15 / 10 / 12 / 0 | Legacy locale IDs, subtags, keyword validation and reflection output |
| MessageFormat | 10 / 15 / 3 / 0 | Argument typing, invalid patterns, reconstruction diagnostics and parse defaults |
| Normalizer | 3 / 0 / 0 / 0 | All exercised fixtures passed |
| Number formatter | 9 / 9 / 6 / 0 | Non-core currency styles, compact values, custom patterns and symbol mutation |
| Resource bundles | 9 / 4 / 1 / 0 | Dimension diagnostics, built-in data coverage and null arguments |
| Spoofchecker | 0 / 11 / 0 / 0 | Spoofchecker is unsupported |
| Time zones | 21 / 24 / 8 / 0 | Canonical aliases, enumeration, object comparison, fixed offsets and diagnostics |
| Transliteration | 3 / 10 / 0 / 0 | Rule creation, inverse transforms, complete ICU ID catalogue and diagnostics |
| IntlChar | 1 / 0 / 0 / 0 | The exercised fixture passed. The property and character-name APIs remain partial. |

## Release measurements

The default uses the full compiled locale data of the linked ICU4X
components. The macOS arm64 CLI measured 33,940,832 bytes before intl,
45,670,080 bytes after Stage 1 and 51,652,480 bytes at the Stage 2 checkpoint
`1f0b20e3`. The Stage 2 increase was 17,711,648 bytes (52.2%). The later
`e7ef1f09` release measured 52,264,736 bytes after standalone resource bundles
and the main-branch process and TLS changes. A 15-locale build would require
exporting and maintaining a separate data bundle. Its size remains unmeasured.

Each loop made 100,000 calls with the formatter reused. These are median
process counters from three runs on macOS arm64 at `e7ef1f09`.
The process counters include startup and the loop. CPU time is user plus
system time. Each currency run produced 1,300,000 bytes and each date run
produced 2,100,000 bytes on both runtimes.

| Operation | PHP instructions / cycles / CPU | Phera instructions / cycles / CPU |
| --- | ---: | ---: |
| `NumberFormatter::formatCurrency` | 1,600 M / 405 M / 0.11 s | 3,025 M / 696 M / 0.20 s |
| `IntlDateFormatter::format` | 3,351 M / 799 M / 0.23 s | 1,239 M / 318 M / 0.09 s |
