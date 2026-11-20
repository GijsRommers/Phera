# Recover PCRE marks with an auxiliary pattern

- Date: 2026-11-20
- Area: PCRE
- Decided by: Design

## Decision

Keep the original PCRE2 match and captures. For patterns with named MARK, PRUNE,
THEN or COMMIT verbs, compile an auxiliary pattern on demand when PHP code
receives a match array. Leave the verbs in place, insert an empty capture after
each verb, and turn branch resets into noncapturing groups. Run the auxiliary
pattern from the original search position, including the options for retrying
an empty match. If its match has the original span, use the participated capture
with the greatest subject offset, breaking ties by position in the pattern.

If the auxiliary pattern fails to compile, fails to match the original span,
or contains numeric references that could be renumbered, omit MARK. For
example, PHP reports MARK `2` for `/(?|(a)(*:1)|(b)(*:2))\1/` on `bb`.
Phera returns the same full match and numbered capture without MARK.
The offset rule can also differ from execution order across assertions.
For `/(?=ab(*:ahead))a(*:after)b/` on `ab`, PHP reports `after` and the
auxiliary pattern selects `ahead`.
When a capture is itself named `MARK`, PHP can append a duplicate key in an
individual match array. Phera stores the verb's mark under the existing key.

## Why

Symfony's compiled route matcher uses MARK to identify a parameterized route.
The safe pcre2 binding does not expose PCRE2's mark accessor, and Phera's crates
forbid unsafe Rust.

## Turned down

Calling the C accessor directly, forking or vendoring the binding, and adding
a second regex engine.
