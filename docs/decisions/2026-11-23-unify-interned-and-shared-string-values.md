# Fold interned strings into PhpString

A PHP string has one variant in each of `Value` and `ArrayKey`.
Interned bytes retain their process-lifetime pointer and precomputed hash,
truthiness and numeric-key classification. Shared bytes retain the existing
copy-on-write vector. The byte-sized string tags follow the eight other
value tags. This gives the compiler consecutive tags for dispatch on
non-string values. Compile-time assertions cover all four 16-byte layouts.

| Structure | Was | Is |
| --- | --- | --- |
| PHP value and array key | Separate variants for interned and shared strings | One `String(PhpString)` variant each |
| String storage | Selected by consumers of the value | A private tag with an interned pointer or a shared vector |
| String borrow | A second representation-specific reference type | A borrowed `PhpString` |
| Array read by string | Separate borrowed lookup methods | One borrowed lookup using the string's hash and numeric-key classification |

This supersedes #216 and #219. The fold keeps main's allocation, hash,
interner policy, and debug output. Separate value variants would require
every extension and fast path to handle both representations.
