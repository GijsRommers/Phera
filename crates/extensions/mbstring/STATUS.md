# mbstring and iconv implementation status

This is an incomplete implementation on `ext/mbstring`, submitted as draft PR #10.
Do not mark it ready while the comparison failures below remain. No comparison was
removed, ignored, or changed to an expected failure.

The crate registers 59 functions, eight case constants, and the two iconv MIME
flags. Runtime registration and the PHP oracle's CI iconv prerequisite are wired.
`mb_send_mail` and the Oniguruma `mb_ereg*` family are deliberately not registered.
`mb_regex_encoding` is a request setting only; it does not imply regex support.

## Encodings and Unicode

48 canonical encoding names and their PHP aliases are advertised. The list in
`data/encodings.txt` is authoritative. These comprise the requested PHP-supported
single-byte families, UTF/UCS variants, Japanese/Korean/Chinese encodings, KOI8,
and transfer encodings. PHP 8.5 itself rejects Windows-1250/1253/1255–1258 and
ISO-8859-11/12 as mbstring names; this implementation also rejects them. Native
iconv can accept additional encodings independently of mbstring's list.

PHP 8.5 uses Unicode 17.0. Case mappings for all eight modes, width, title-case
properties, single-byte tables, and CJK mappings were generated from real PHP
8.5.12, rather than the newer Unicode data in the local php-src main checkout.
The generator programs are committed. No runtime operation invokes PHP.
GB18030 has 39 explicit corrections for PHP's 2000 mappings. SJIS, CP932, and
SJIS-win retain distinct PHP tables. Valid representable-character round trips
pass for all 44 character encodings. Stateful malformed ISO-2022-JP and arbitrary
GB18030 invalid-sequence behavior need more work and broader differential tests.
The four transfer encodings have their own PHP comparison matrix.

## Function inventory

DONE means implemented with the reported comparison coverage. It is not a claim
that the requested exhaustive options/subjects cross-product is complete for
every function; remaining shared codec and coverage limitations above still apply.

| Function | Status | Remaining work / evidence |
| --- | --- | --- |
| mb_strlen | DONE | Script/invalid/large matrices, Unicode and byte-pair sweeps. |
| mb_substr | DONE | Offsets, lengths, byte preservation, large inputs. |
| mb_strpos | DONE | Forward search and offset validation. |
| mb_strrpos | DONE | Reverse search and offset validation. |
| mb_stripos | DONE | PHP simple-fold search. |
| mb_strripos | DONE | PHP simple-fold reverse search. |
| mb_strstr | DONE | Before/after needle variants. |
| mb_strrchr | DONE | Last-needle variants. |
| mb_stristr | DONE | Case-insensitive before/after variants. |
| mb_strrichr | DONE | Case-insensitive last-needle variants. |
| mb_substr_count | DONE | Counts and empty-needle errors. |
| mb_str_split | DONE | Length validation and malformed-byte boundaries. |
| mb_strtoupper | DONE | Whole-Unicode mapping and context/transfer matrices. |
| mb_strtolower | DONE | Whole-Unicode mapping and final sigma. |
| mb_convert_case | DONE | All eight modes; whole-Unicode and contextual cases. |
| mb_ucfirst | DONE | First-character expansion and remainder preservation. |
| mb_lcfirst | DONE | First-character mapping and remainder preservation. |
| mb_trim | DONE | Default/custom character lists and malformed bytes. |
| mb_ltrim | DONE | Left-only variant. |
| mb_rtrim | DONE | Right-only variant. |
| mb_str_pad | DONE | Length, pad string, side and validation order. |
| mb_strwidth | DONE | Every Unicode code point compared. |
| mb_strimwidth | DONE | Marker, offsets, negative-width deprecation. |
| mb_strcut | PARTIAL | ISO-2022-JP byte budgets and shift-state slicing need correction. |
| mb_convert_encoding | PARTIAL | Nested references/objects, language-specific auto lists. |
| mb_detect_encoding | PARTIAL | Basic PHP scoring; full heuristics/BOM/order edge cases missing. |
| mb_check_encoding | DONE | Unicode, invalid byte pairs, arrays, recursion guard. |
| mb_scrub | DONE | Invalid units, substitute modes and transfer formats tested. |
| mb_ord | DONE | Whole Unicode and unsupported-encoding errors. |
| mb_chr | DONE | Whole Unicode and every representable encoding scalar. |
| mb_internal_encoding | DONE | Request-local getter/setter. |
| mb_substitute_character | DONE | Numeric, none, long, entity; numeric setting retained. |
| mb_detect_order | PARTIAL | Language-specific auto presets not fully implemented. |
| mb_language | PARTIAL | State/aliases implemented; language-dependent defaults incomplete. |
| mb_http_input | PARTIAL | Default/no-transcoding results only; no request-input detection hook. |
| mb_http_output | DONE | Request-local getter/setter including pass. |
| mb_regex_encoding | DONE | Getter/setter only; no regex functions registered. |
| mb_preferred_mime_name | DONE | PHP MIME names for supported encodings. |
| mb_encoding_aliases | DONE | PHP aliases for supported encodings. |
| mb_list_encodings | DONE | Advertises only supported names; differs from full PHP's 79 names. |
| mb_encode_numericentity | DONE | Range maps, offsets, masks, decimal/hex. |
| mb_decode_numericentity | DONE | Numeric references and range mappings. |
| mb_convert_variables | PARTIAL | Nested object/reference mutation and full recursive parity missing. |
| mb_encode_mimeheader | PARTIAL | Language defaults and stateful charset folding need broader coverage. |
| mb_decode_mimeheader | PARTIAL | Common B/Q/unfolding pass; malformed-word coverage incomplete. |
| mb_get_info | PARTIAL | Illegal-character counter and language mail metadata incomplete. |
| mb_parse_str | PARTIAL | Basic arrays/references pass; configurable ini/input translation missing. |
| mb_output_handler | PARTIAL | Registered; buffering status/content-type lifecycle untested end to end. |
| mb_convert_kana | DONE | Width, voiced marks, all flag pairs and errors across six encodings. |
| iconv | PARTIAL | Native mappings and IGNORE; macOS TRANSLIT comparison failures remain. |
| iconv_strlen | DONE | Native conversion, invalid/incomplete diagnostics. |
| iconv_substr | DONE | Native character offsets and optional length. |
| iconv_strpos | DONE | Offset/error precedence and early-match behavior in the matrix. |
| iconv_strrpos | DONE | Reverse search and invalid/incomplete diagnostics. |
| iconv_mime_encode | PARTIAL | Basic B/Q/options; character-safe folding needs fuller coverage. |
| iconv_mime_decode | PARTIAL | Flags/folding/common words pass; malformed-state coverage incomplete. |
| iconv_mime_decode_headers | PARTIAL | Repeated fields pass; non-ASCII field names/charsets incomplete. |
| iconv_get_encoding | DONE | Request-local all/individual settings. |
| iconv_set_encoding | DONE | Request-local updates and deprecation diagnostics. |
| mb_send_mail | NOT DONE | Explicitly out of scope; not registered. |
| mb_ereg* | NOT DONE | Explicitly out of scope; nothing registered. |

## Verification

Final local PHP 8.5.12 run: **41,015,229 compared calls, 24 mismatches**.

| Suite | Compared calls | Mismatches |
| --- | ---: | ---: |
| Every Unicode code point, eight case modes, width, ord/chr/check | 15,570,944 | 0 |
| Every representable character across 44 encodings | 25,091,286 | 0 |
| Every two-byte UTF-8 input, four functions | 262,144 | 0 |
| Script, malformed-byte, search and slicing matrix | 70,452 | 0 |
| Kana options across six encodings | 9,330 | 0 |
| Padding/entities/MIME/detection/iconv options | 7,962 | 3 |
| All registered functions with hostile arguments | 1,239 | 21 |
| Transfer encoding matrix | 1,512 | 0 |
| Request settings | 237 | 0 |
| 1 MB subjects across six encodings | 114 | 0 |
| By-reference output comparisons | 9 | 0 |

The suites use `find_php`/`run_php` and capture values, output, diagnostics and
by-reference results. The Unicode and representable-character sweeps account for
40,662,230 compared calls before the option and invalid-byte matrices.

The literal-argument hostile matrix includes 14 engine/API mismatches: PHP rejects
non-variable arguments to by-reference parameters before entering the extension;
`testing::call` / `prepare_arguments` do not perform that call-site check. Seven
additional comparisons call `mb_list_encodings` and compare its full array to
PHP's larger list. Those cases remain present and failing; unsupported encodings
have not been falsely advertised to make the comparison pass.

On macOS, `//TRANSLIT` differs for the Windows-1252 interpretations of the UTF-8
byte strings `éßİΟΣ`, `日本語`, and `العربية`. Both PHP and the dependency link the
system libiconv, but observed output changes between runs. No macOS output is
pinned as a cross-platform expected value. Linux uses glibc through native iconv;
a stable glibc-versus-macOS difference catalogue and a green Linux gate are still
required. The same-host oracle comparisons remain enabled on Linux and macOS.

Release in-process benchmark, seven samples of 30 calls, median per call,
1,048,565 UTF-8 bytes (ASCII, accented Latin, Greek, Chinese and emoji):

| Function | PHP | Phera native | Phera / PHP |
| --- | ---: | ---: | ---: |
| mb_strlen | 0.808 ms | 0.495 ms | 0.61 |
| mb_substr | 1.139 ms | 1.470 ms | 1.29 |
| mb_strtoupper | 3.880 ms | 7.089 ms | 1.83 |

These measure native function calls within the process, excluding startup and
using warm tables. They are not full VM or production throughput measurements.
Run `cargo run --release -p phera-ext-mbstring --example benchmark` to reproduce.

## Dependencies and engine integration

- `phera-extension-api`: registration, argument handling, request state and oracle.
- `encoding_rs` 0.8.44: pure Rust web-codec fallback, with PHP mapping corrections.
- `base64` 0.22.1: Base64 output and RFC 2047 MIME transfer primitives.
- `iconv-native` 0.1.0: safe interface to the platform C iconv. This wrapper is
  necessary because pure Rust codecs do not reproduce libc's TRANSLIT mappings.
  No unsafe code is introduced into the workspace.

Settings live in `RequestState`. A runtime request test verifies registration,
by-reference parsing and isolation across two requests. Missing integration is
PHP ini-backed defaults/limits, request-input encoding detection, accurate
illegal-character accounting, and output-buffer status/content-type handling.
Full nested-object mutation semantics also need further extension/VM work.

`make ci` was not run locally because its database setup is outside this task's
permission boundary. Runtime compilation, the focused runtime request test,
crate Clippy, workspace formatting and the unsafe-code guard were run separately.
The PR remains draft; it has not been merged or marked ready.
