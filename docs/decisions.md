# Design decisions

## 2026-11-19 — Intl uses ICU4X compiled data

Use ICU4X 2.4 with its compiled Unicode 18 and CLDR 49 data. Link the
components the extension calls and keep their full compiled locale coverage
by default. A locale subset would save space but would silently change results
for applications outside the acceptance locales and would require a custom data
build. Measure the release binary at the Stage 1 gate so that trade-off has a
number attached to it. Linking ICU4C would add a system dependency and still
leave results dependent on the host's ICU and CLDR versions.

Compare types, reference mutations, diagnostics and locale-independent results
exactly with PHP 8.5. Compare locale data with PHP on the test host across the
core locales and record individually reviewed exceptions with both CLDR versions
in the [intl compatibility notes](../crates/extensions/intl/README.md).
A single machine's formatted output would pin incidental ICU4C locale data for
every platform.

## 2026-11-19 — Intl keeps Unicode scalars and PHP bytes distinct

Use ICU4X canonical normalization plus the Unicode Character Database's raw
compatibility and NFKC case-fold mappings. PHP exposes these mappings directly;
recursive compatibility decomposition loses intermediate scalars. Keep the
table in compact text and load it on first use. Rejected approximating case
folding with lowercase or normalizing the raw decomposition before returning it.

Use ICU4X's Unicode 18 grapheme state machine for boundaries and retain the
original PHP bytes for UTF-8 text that PHP's iterator accepts lossily. PHP's
UTF-16 conversion paths reject invalid UTF-8 and return their own diagnostics.
For search, compare collation keys at grapheme boundaries, with ICU-equivalent
compatibility expansions and a prefix table to avoid repeatedly rescanning the
haystack. ICU4X has no ICU4C StringSearch API; locale-specific contractions
across clusters still need coverage.

## 2026-11-19 — Intl state is request-scoped

Store the last intl error in the extension API's request store and the INI
settings in the runtime's INI catalog. That keeps concurrent requests isolated
and gives PHP INI reads and updates the same setting source. Rejected
process-global error and locale state or a second copy of the INI values.
