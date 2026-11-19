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
| `intl_*error*`, `intl_is_failure`, `IntlException` | Request-local last error, ICU error names and numeric compatibility constants |

The two normalization mapping files under `data/` are derived from Unicode
18.0.0 [UnicodeData.txt](https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt)
(decomposition field, compatibility rows) and
[DerivedNormalizationProps.txt](https://www.unicode.org/Public/18.0.0/ucd/DerivedNormalizationProps.txt)
(`NFKC_CF` property). The Unicode data license is included alongside them.
The ICU numeric constants and error names are the public PHP intl and ICU
error-code tables as exposed by PHP 8.5.12 with ICU 79.1.

## Locale data differences

| Call | PHP / ICU4C CLDR | Phera / ICU4X CLDR | Difference and disposition |
| --- | --- | --- | --- |
| Locale formatting and display-name matrix | 49 on the Mac oracle | 49 | Pending Stage 1 formatting and locale APIs |
