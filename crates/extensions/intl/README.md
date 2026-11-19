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

The two normalization mapping files under `data/` are derived from Unicode
18.0.0 [UnicodeData.txt](https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt)
(decomposition field, compatibility rows) and
[DerivedNormalizationProps.txt](https://www.unicode.org/Public/18.0.0/ucd/DerivedNormalizationProps.txt)
(`NFKC_CF` property). The Unicode data license is included alongside them.
The ICU numeric constants and error names are the public PHP intl and ICU
error-code tables as exposed by PHP 8.5.12 with ICU 79.1.

CLDR 49's number patterns, locale display patterns, Gregorian date styles and
currency-region map are compiled into the extension. Their source is the
[CLDR JSON release](https://github.com/unicode-org/cldr-json/tree/49.0.0).
The [IANA Language Subtag Registry](https://www.iana.org/assignments/language-subtag-registry/)
supplies grandfathered tags.

## Locale data differences

| Call | PHP / ICU4C CLDR | Phera / ICU4X CLDR | Difference and disposition |
| --- | --- | --- | --- |
| Decimal, currency and percent grouping of four-digit numbers in `es_ES` and `it_IT` | 49 | 49 | Fixed. PHP's legacy formatter groups before ICU4X's default minimum. |
| Half-integer `JPY` values in `ja_JP` | 49 | 49 | Fixed. Round half-even before the ICU4X currency formatter applies its precision. |
| Long date-time style in `ar_EG`, Brussels GMT offsets at Unix timestamps 0 and 1724727600 | 49 | 49 | Fixed. Match ICU4C's unmarked sign in the rendered Arabic time-zone field. |

On macOS, all 8,646 Stage 1 differential cases pass with PHP 8.5.12 and
ICU4C 79.1. That includes 675 display-name comparisons and 750 number and date
format comparisons across the 15 core locales. The wider PHP intl suite
covers additional patterns and API combinations.
