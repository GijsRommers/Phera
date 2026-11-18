# Tokenizer

`token_get_all()` and `PhpToken::tokenize()` share Phera’s byte scanner. The
extension oracle compares PHP 8.5 token IDs, text, lines, byte offsets, errors
and diagnostics. It includes the repository’s PHPT source and binary inputs.

## Scanner diagnostics

Audit of the error calls in PHP 8.5.12’s `Zend/zend_language_scanner.l`. Line
numbers locate the calls in the `origin/PHP-8.5.12` ref.

| Lines | PHP diagnostic | Phera path |
| --- | --- | --- |
| 310 | Cannot use `<?=` as an identifier | Parser identifier check, including after a close tag in a trait alias |
| 556, 759, 895 | Could not convert the script from the detected encoding | Requires Zend’s optional multibyte script encoding filters. Phera scans the input bytes directly. |
| 566 | `zend_stream_mmap() failed` | Specific to Zend’s mmap source loader. Phera uses a filesystem read and reports its I/O error. |
| 1063, 1075 | Invalid UTF-8 codepoint escape sequence, optionally with “Codepoint too large” | Double-quoted and heredoc escape parsing |
| 1114 | Octal escape sequence overflow | `E_COMPILE_WARNING` through `Context`. Raw scanning uses the caller’s location. Parsed scanning uses the tokenized source or compiled file and line. |
| 1208, 1215, 2732, 2802, 3006, 3130 | Invalid heredoc or nowdoc indentation, including mixed tabs and spaces | Heredoc parsing and scanner diagnostics |
| 1270, 1282 | Unclosed, mismatched or unmatched delimiters | Shared scanner nesting diagnostics in parsed mode |
| 1642, 1656, 1678, 1700 | Non-canonical casts `(integer)`, `(double)`, `(binary)`, `(boolean)` | `E_DEPRECATED` with scanned source location in parsed mode |
| 1666 | The removed `(real)` cast | `ParseError` in parsed mode |
| 2117 | Invalid numeric literal, including legacy octal with an 8 or 9 | Numeric literal parsing |
| 2501 | Unterminated comment starting line | Parsed lexer error |

Raw tokenization suppresses scanner parse exceptions. Raw heredoc body text
stays undecoded, while a quoted string inside an interpolation still produces
its own warning. PHP’s fixture `warning_during_heredoc_scan_ahead.phpt` checks
both behaviors in the oracle. The runnable PHPTs in
`tests/fixtures/scanner-diagnostics` also exercise the CLI and both native APIs.

Native deprecation handlers receive the scanned location. If a handler throws,
a later queued scanner warning is discarded. The native caller remains in
the exception trace, as exercised by `throwing-cast-handler.phpt` and php-src’s
`gh19507_throw.phpt`.

The file loader preserves literal bytes and inline HTML through compilation.
Static identifier references also work for non-UTF-8 names. Byte-exact
dynamic references, name introspection and diagnostics containing arbitrary
identifier bytes require byte strings in the VM’s name and exception tables.

## Engine-dependent behavior

- **Called class in a native static call.** `ChildToken::tokenize()` must
  construct `ChildToken` objects, populate its default properties, and
  validate abstract classes. php-src’s `PhpToken_extension.phpt` and
  `PhpToken_extension_errors.phpt` exercise this.
- **Typed uninitialized native properties.** Native property declarations
  need a PHP type and an initially uninitialized state. A regression should
  assert that `$token = new PhpToken(T_STRING, 'a'); $token->id = [];` raises a
  `TypeError`, and that `unset($token->text); $token->text` raises the typed
  property initialization error. PHP object writes and direct reads own both
  checks.
- **Implicit object string conversion.** The VM must dispatch `__toString()`
  for casts and string-consuming operations such as `implode()`. Use
  php-src’s `PhpToken_toString.phpt` for this behavior.

Phera’s lexer supports the short-tag setting. The native tokenizer uses PHP’s
ini-free default until the runtime exposes `short_open_tag` to extensions.
php-src’s `no_inline_html_split.phpt` exercises the disabled setting.
